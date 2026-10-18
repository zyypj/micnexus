package app.micnexus.android

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.math.sin

/**
 * The phone's [Session] against the real Windows core. Opt-in, needs the PC
 * side running on the same network:
 *
 *   cargo run --example headless -- 20                       (apps/desktop/src-tauri)
 *   MICNEXUS_INTEROP=1 ./gradlew :app:testDebugUnitTest --tests '*InteropTest*'
 *
 * Answers discovery like Engine does, accepts the PC's TCP connection and
 * streams 3 s of a 440 Hz tone; headless prints what reached the cable.
 */
class InteropTest {
    @Test
    fun streamsToTheWindowsCore() {
        assumeTrue(System.getenv("MICNEXUS_INTEROP") == "1")
        val server = ServerSocket()
        server.reuseAddress = true
        server.bind(InetSocketAddress(Protocol.PHONE_PORT))
        val udp = DatagramSocket(null).apply {
            reuseAddress = true
            bind(InetSocketAddress(Protocol.DISCOVERY_PORT))
            soTimeout = 15_000
        }
        val discovery = Thread {
            val buf = ByteArray(2048)
            try {
                while (true) {
                    val p = DatagramPacket(buf, buf.size)
                    udp.receive(p)
                    val info = JSONObject(String(p.data, p.offset + 4, p.length - 4))
                    val reply = JSONObject().put("v", 1).put("id", "kotlin-phone").put("name", "Kotlin Session")
                        .put("model", "JVM").put("port", Protocol.PHONE_PORT).put("want", info.getString("id"))
                    val bytes = Protocol.DISCOVER_REPLY + reply.toString().toByteArray()
                    udp.send(DatagramPacket(bytes, bytes.size, p.socketAddress))
                }
            } catch (_: Exception) {
            }
        }.apply { isDaemon = true; start() }

        val socket = server.accept()
        socket.tcpNoDelay = true
        val info = Protocol.read(socket.getInputStream())
        assertEquals(Protocol.INFO, info.kind)
        val welcome = CountDownLatch(1)
        val stats = CountDownLatch(2)
        val ended = CountDownLatch(1)
        var codec = ""
        var buffer = -1
        val session = Session(socket.getInputStream(), socket.getOutputStream(), { socket.close() }, "wifi", "pc", "PC", "lan",
            object : Session.Events {
                override fun onPending(s: Session) {}
                override fun onWelcome(s: Session, c: String, rate: Int, frameMs: Int) {
                    codec = "$c/$rate/$frameMs"
                    welcome.countDown()
                }
                override fun onMuteRequest(s: Session, muted: Boolean) {}
                override fun onStats(s: Session, bufferMs: Int, rttMs: Int) {
                    buffer = bufferMs
                    stats.countDown()
                }
                override fun onEnded(s: Session, reason: String) = ended.countDown()
            })
        session.start(JSONObject().put("v", 1).put("id", "kotlin-phone").put("name", "Kotlin Session").put("model", "JVM")
            .put("version", "test").put("codecs", JSONArray().put("pcm16")).put("muted", false))
        assertTrue("no WELCOME", welcome.await(10, TimeUnit.SECONDS))
        assertEquals("pcm16/48000/10", codec)

        var phase = 0.0
        val start = System.nanoTime()
        for (seq in 0 until 300) {
            val pcm = ByteArray(960)
            for (i in 0 until 480) {
                val v = (sin(phase) * 0.5 * 32767).toInt()
                pcm[2 * i] = v.toByte()
                pcm[2 * i + 1] = (v shr 8).toByte()
                phase += 2 * Math.PI * 440 / 48000
            }
            session.send(Protocol.AUDIO, Protocol.le32(seq) + pcm)
            val due = start + (seq + 1) * 10_000_000L
            val wait = due - System.nanoTime()
            if (wait > 0) Thread.sleep(wait / 1_000_000, (wait % 1_000_000).toInt())
        }
        assertTrue("no STATS", stats.await(5, TimeUnit.SECONDS))
        assertTrue("PC buffer $buffer ms", buffer in 1..200)
        session.bye("user")
        ended.await(3, TimeUnit.SECONDS)
        udp.close()
        server.close()
        discovery.join(1000)
    }
}
