package app.micnexus.android

import java.io.DataInputStream
import java.io.EOFException
import java.io.InputStream
import java.util.UUID

/** Wire format shared with the Windows app (docs/PROTOCOL.md). */
object Protocol {
    const val VERSION = 1
    /** TCP port this phone listens on (Wi‑Fi, and USB through `adb forward`). */
    const val PHONE_PORT = 47820
    /** UDP port that answers the PC's discovery. */
    const val DISCOVERY_PORT = 47821
    val BLUETOOTH_UUID: UUID = UUID.fromString("a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a15")

    val DISCOVER_REQUEST = "MNX?".toByteArray()
    val DISCOVER_REPLY = "MNX!".toByteArray()

    const val HELLO = 1
    const val WELCOME = 2
    const val PENDING = 3
    const val AUDIO = 4
    const val MUTE = 5
    const val PING = 6
    const val PONG = 7
    const val BYE = 8
    const val INFO = 10
    const val STATS = 11

    class Frame(val kind: Int, val payload: ByteArray)

    fun encode(kind: Int, payload: ByteArray): ByteArray {
        val len = payload.size.coerceAtMost(0xFFFF)
        val out = ByteArray(3 + len)
        out[0] = kind.toByte()
        out[1] = (len and 0xFF).toByte()
        out[2] = (len shr 8).toByte()
        System.arraycopy(payload, 0, out, 3, len)
        return out
    }

    fun read(input: InputStream): Frame {
        val data = if (input is DataInputStream) input else DataInputStream(input)
        val kind = data.read()
        if (kind < 0) throw EOFException()
        val lo = data.readUnsignedByte()
        val hi = data.readUnsignedByte()
        val payload = ByteArray(lo or (hi shl 8))
        data.readFully(payload)
        return Frame(kind, payload)
    }

    fun le32(v: Int): ByteArray =
        byteArrayOf(v.toByte(), (v shr 8).toByte(), (v shr 16).toByte(), (v shr 24).toByte())

    fun le64(v: Long): ByteArray = ByteArray(8) { i -> (v shr (8 * i)).toByte() }

    fun readLe64(b: ByteArray, at: Int = 0): Long {
        var v = 0L
        for (i in 7 downTo 0) v = (v shl 8) or (b[at + i].toLong() and 0xFF)
        return v
    }
}
