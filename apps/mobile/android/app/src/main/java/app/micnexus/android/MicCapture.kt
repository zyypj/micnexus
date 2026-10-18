package app.micnexus.android

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AudioEffect
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import android.os.Process
import android.os.SystemClock
import android.util.Log
import kotlin.math.abs

/**
 * Microphone → (gain) → PCM 16-bit or Opus → AUDIO payloads (`u32 seq` + data).
 * Runs on its own high-priority thread; [stop] releases the microphone (the
 * Android privacy indicator goes off while muted).
 */
class MicCapture(
    private val codec: String,
    private val rate: Int,
    private val frameMs: Int,
    private val noiseSuppression: Boolean,
    private val gain: Float,
    private val send: (ByteArray) -> Unit,
    private val onLevel: (Float) -> Unit,
    private val onError: (String) -> Unit,
) {
    @Volatile private var running = false
    private var thread: Thread? = null

    fun start() {
        if (running) return
        running = true
        thread = Thread({ run() }, "mic-capture").also { it.start() }
    }

    fun stop() {
        running = false
        thread?.join(500)
        thread = null
        onLevel(0f)
    }

    @SuppressLint("MissingPermission")
    private fun run() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        val frame = rate * frameMs / 1000
        val minBuf = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val source = if (noiseSuppression) MediaRecorder.AudioSource.VOICE_COMMUNICATION else MediaRecorder.AudioSource.MIC
        val record = try {
            AudioRecord(source, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(minBuf, frame * 2 * 4))
        } catch (e: Exception) {
            onError("Não foi possível abrir o microfone: ${e.message}")
            running = false
            return
        }
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            record.release()
            onError("O microfone está ocupado por outro app")
            running = false
            return
        }
        val effects = mutableListOf<AudioEffect>()
        if (noiseSuppression) {
            if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(record.audioSessionId)?.let { it.enabled = true; effects += it }
            if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(record.audioSessionId)?.let { it.enabled = true; effects += it }
            if (AutomaticGainControl.isAvailable()) AutomaticGainControl.create(record.audioSessionId)?.let { it.enabled = true; effects += it }
        }
        val encoder = if (codec == "opus") OpusEncoder(rate, frame) else null
        val samples = ShortArray(frame)
        val pcm = ByteArray(frame * 2)
        var seq = 0
        var peak = 0f
        var lastLevel = 0L
        try {
            record.startRecording()
            while (running) {
                var got = 0
                while (got < frame && running) {
                    val n = record.read(samples, got, frame - got)
                    if (n < 0) throw IllegalStateException("AudioRecord.read = $n")
                    got += n
                }
                if (!running) break
                for (i in 0 until frame) {
                    val v = (samples[i] * gain).toInt().coerceIn(-32768, 32767)
                    pcm[2 * i] = v.toByte()
                    pcm[2 * i + 1] = (v shr 8).toByte()
                    val a = abs(v) / 32768f
                    if (a > peak) peak = a
                }
                val now = SystemClock.elapsedRealtime()
                if (now - lastLevel >= 50) {
                    onLevel(peak)
                    peak = 0f
                    lastLevel = now
                }
                if (encoder == null) {
                    send(Protocol.le32(seq++) + pcm)
                } else {
                    encoder.encode(pcm) { packet -> send(Protocol.le32(seq++) + packet) }
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "capture stopped", e)
            if (running) onError("O microfone parou: ${e.message}")
        } finally {
            running = false
            try { record.stop() } catch (_: Exception) {}
            record.release()
            effects.forEach { it.release() }
            encoder?.release()
        }
    }

    /** MediaCodec Opus encoder (Android 10+): one 20 ms PCM frame in, one Opus packet out. */
    private class OpusEncoder(rate: Int, private val frame: Int) {
        private val codec: MediaCodec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
        private val info = MediaCodec.BufferInfo()
        private var ptsUs = 0L
        private val frameUs = frame * 1_000_000L / rate

        init {
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, rate, 1).apply {
                setInteger(MediaFormat.KEY_BIT_RATE, 64_000)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, frame * 2)
            }
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            codec.start()
        }

        fun encode(pcm: ByteArray, out: (ByteArray) -> Unit) {
            val inIdx = codec.dequeueInputBuffer(10_000)
            if (inIdx >= 0) {
                val buf = codec.getInputBuffer(inIdx)!!
                buf.clear()
                buf.put(pcm, 0, minOf(pcm.size, buf.remaining()))
                codec.queueInputBuffer(inIdx, 0, pcm.size, ptsUs, 0)
                ptsUs += frameUs
            }
            while (true) {
                val outIdx = codec.dequeueOutputBuffer(info, 0)
                if (outIdx < 0) break
                if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0 && info.size > 0) {
                    val buf = codec.getOutputBuffer(outIdx)!!
                    val packet = ByteArray(info.size)
                    buf.position(info.offset)
                    buf.get(packet)
                    out(packet)
                }
                codec.releaseOutputBuffer(outIdx, false)
            }
        }

        fun release() {
            try { codec.stop() } catch (_: Exception) {}
            codec.release()
        }
    }

    companion object {
        private const val TAG = "MicCapture"

        /** Whether this phone has an Opus encoder (MediaCodec, Android 10+). */
        val opusAvailable: Boolean by lazy {
            try {
                MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.any { info: MediaCodecInfo ->
                    info.isEncoder && info.supportedTypes.any { it.equals(MediaFormat.MIMETYPE_AUDIO_OPUS, true) }
                }
            } catch (_: Exception) {
                false
            }
        }
    }
}
