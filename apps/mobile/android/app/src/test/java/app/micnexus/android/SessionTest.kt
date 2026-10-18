package app.micnexus.android

import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.DataInputStream
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * The phone side of docs/PROTOCOL.md against a scripted PC that writes the
 * same bytes as apps/desktop/src-tauri/src/protocol.rs.
 */
class SessionTest {
    @Test
    fun framesMatchTheRustEncoding() {
        // protocol.rs: encode_frame(AUDIO, [1,2,3]) == [4, 3, 0, 1, 2, 3]
        assertArrayEquals(byteArrayOf(4, 3, 0, 1, 2, 3), Protocol.encode(Protocol.AUDIO, byteArrayOf(1, 2, 3)))
        val f = Protocol.read(ByteArrayInputStream(byteArrayOf(5, 1, 0, 1)))
        assertEquals(Protocol.MUTE, f.kind)
        assertArrayEquals(byteArrayOf(1), f.payload)
        assertEquals(0x0403_0201L, Protocol.readLe64(Protocol.le64(0x0403_0201L)))
        assertArrayEquals(byteArrayOf(1, 0, 0, 0), Protocol.le32(1))
    }

    @Test
    fun handshakeAudioMuteAndBye() {
        ServerSocket(0).use { server ->
            val events = mutableListOf<String>()
            val welcomed = CountDownLatch(1)
            val ended = CountDownLatch(1)
            val muteAsked = CountDownLatch(1)
            var endReason = ""
            lateinit var session: Session

            // The "phone" connects (Bluetooth direction) and reads INFO first, like Engine does.
            val pc = Thread {
                server.accept().use { s ->
                    val out = s.getOutputStream()
                    val input = DataInputStream(s.getInputStream())
                    out.write(Protocol.encode(Protocol.INFO, """{"v":1,"id":"pc","name":"PC","version":"0.1.0","busy":false}""".toByteArray()))
                    val hello = Protocol.read(input)
                    assertEquals(Protocol.HELLO, hello.kind)
                    val h = JSONObject(String(hello.payload))
                    assertEquals("phone-id", h.getString("id"))
                    assertTrue(h.getJSONArray("codecs").toString().contains("pcm16"))
                    out.write(Protocol.encode(Protocol.PENDING, ByteArray(0)))
                    out.write(Protocol.encode(Protocol.WELCOME, """{"v":1,"id":"pc","name":"PC","version":"0.1.0","codec":"pcm16","rate":48000,"frameMs":10}""".toByteArray()))
                    // Expect one AUDIO frame (seq + 960 bytes) from the test below.
                    var audio: Protocol.Frame
                    do audio = Protocol.read(input) while (audio.kind != Protocol.AUDIO)
                    assertEquals(4 + 960, audio.payload.size)
                    // PC asks to mute; then says goodbye.
                    out.write(Protocol.encode(Protocol.MUTE, byteArrayOf(1)))
                    out.write(Protocol.encode(Protocol.STATS, """{"bufferMs":30,"underruns":0}""".toByteArray()))
                    muteAsked.await(5, TimeUnit.SECONDS)
                    out.write(Protocol.encode(Protocol.BYE, """{"reason":"user"}""".toByteArray()))
                    Thread.sleep(200)
                }
            }.apply { start() }

            val socket = Socket("127.0.0.1", server.localPort)
            val info = Protocol.read(socket.getInputStream())
            assertEquals(Protocol.INFO, info.kind)
            session = Session(
                socket.getInputStream(),
                socket.getOutputStream(),
                { socket.close() },
                "bluetooth",
                "pc",
                "PC",
                "AA:BB",
                object : Session.Events {
                    override fun onPending(s: Session) { synchronized(events) { events += "pending" } }
                    override fun onWelcome(s: Session, codec: String, rate: Int, frameMs: Int) {
                        synchronized(events) { events += "welcome:$codec:$rate:$frameMs" }
                        welcomed.countDown()
                    }
                    override fun onMuteRequest(s: Session, muted: Boolean) {
                        synchronized(events) { events += "mute:$muted" }
                        muteAsked.countDown()
                    }
                    override fun onStats(s: Session, bufferMs: Int, rttMs: Int) { synchronized(events) { events += "stats:$bufferMs" } }
                    override fun onEnded(s: Session, reason: String) {
                        endReason = reason
                        ended.countDown()
                    }
                },
            )
            session.start(JSONObject().put("v", 1).put("id", "phone-id").put("name", "Test").put("codecs", org.json.JSONArray().put("pcm16")).put("muted", false))
            assertTrue(welcomed.await(5, TimeUnit.SECONDS))
            session.send(Protocol.AUDIO, Protocol.le32(0) + ByteArray(960))
            assertTrue(ended.await(5, TimeUnit.SECONDS))
            pc.join(2000)
            assertEquals("user", endReason)
            val seen = synchronized(events) { events.toList() }
            assertEquals(listOf("pending", "welcome:pcm16:48000:10", "mute:true", "stats:30"), seen)
        }
    }
}
