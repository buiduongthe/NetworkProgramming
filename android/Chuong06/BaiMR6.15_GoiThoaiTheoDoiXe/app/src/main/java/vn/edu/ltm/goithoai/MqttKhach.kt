package vn.edu.ltm.goithoai

import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.IOException
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import kotlin.concurrent.thread

/**
 * Bài mở rộng MR6.15 – MQTT client tối giản (MQTT 3.1.1, QoS 0) viết trên Socket TCP, cùng cách mã hóa gói với
 * broker của máy chủ (mqtt_broker.py / mqtt.go). Không dùng thư viện ngoài để thấy rõ giao thức:
 *   byte đầu = (loại << 4) | cờ; "độ dài còn lại" mã hóa 7 bit/byte; chuỗi UTF-8 có tiền tố độ dài 2 byte.
 * Tài xế: ketNoi(..., diChuc = xe/<mã>/trangthai = "mat_ket_noi") rồi phat(xe/<mã>/vitri, JSON, retain = true).
 * Khách: dangKy("xe/<mã>/vitri", "xe/<mã>/trangthai"); tin tới qua khiNhan (trên luồng đọc – tự chuyển về luồng
 * giao diện nếu cần). Mọi hàm mạng chặn (blocking): gọi trên luồng nền.
 */
class MqttKhach(
    private val maKhach: String,
    private val khiNhan: (chuDe: String, noiDung: ByteArray, giuLai: Boolean) -> Unit,
    private val khiMat: (lyDo: String) -> Unit = {},
    private val keepalive: Int = 15,
) {
    private var socket: Socket? = null
    private var ra: OutputStream? = null
    private val khoaGhi = Any()
    private var maGoi = 0
    @Volatile var dangMo = false
        private set

    companion object {
        const val CONNECT = 1; const val CONNACK = 2; const val PUBLISH = 3; const val SUBSCRIBE = 8
        const val SUBACK = 9; const val PINGREQ = 12; const val DISCONNECT = 14

        fun maDoDai(n0: Int): ByteArray {
            val kq = ByteArrayOutputStream()
            var n = n0
            do {
                var b = n % 128
                n /= 128
                if (n > 0) b = b or 0x80
                kq.write(b)
            } while (n > 0)
            return kq.toByteArray()
        }

        fun chuoi(s: String): ByteArray {
            val b = s.toByteArray(Charsets.UTF_8)
            return byteArrayOf((b.size shr 8).toByte(), b.size.toByte()) + b
        }

        fun goi(loai: Int, co: Int, than: ByteArray): ByteArray =
            byteArrayOf(((loai shl 4) or co).toByte()) + maDoDai(than.size) + than
    }

    /** Kết nối và chờ CONNACK (tối đa 5 s). diChuc = (chủ đề, nội dung, retain): Last Will. Lỗi -> IOException. */
    fun ketNoi(host: String, cong: Int, diChuc: Triple<String, ByteArray, Boolean>? = null) {
        val s = Socket()
        s.connect(InetSocketAddress(host, cong), 5000)
        s.soTimeout = 5000
        s.tcpNoDelay = true
        socket = s
        ra = s.getOutputStream()
        var co = 0x02                                        // clean session
        var tai = chuoi(maKhach)
        if (diChuc != null) {
            co = co or 0x04 or (if (diChuc.third) 0x20 else 0)
            tai += chuoi(diChuc.first) + byteArrayOf((diChuc.second.size shr 8).toByte(), diChuc.second.size.toByte()) +
                diChuc.second
        }
        val than = chuoi("MQTT") + byteArrayOf(4, co.toByte(), (keepalive shr 8).toByte(), keepalive.toByte()) + tai
        ghi(goi(CONNECT, 0, than))
        val vao = DataInputStream(s.getInputStream())
        val (loai, _, t) = docGoi(vao)
        if (loai != CONNACK || t.size < 2 || t[1].toInt() != 0) {
            s.close()
            throw IOException("broker từ chối kết nối (mã ${if (t.size > 1) t[1] else -1})")
        }
        s.soTimeout = 0
        dangMo = true
        thread(name = "mqtt-doc", isDaemon = true) { vongDoc(vao) }
        thread(name = "mqtt-ping", isDaemon = true) {
            try {
                while (dangMo) {
                    Thread.sleep(keepalive * 800L)
                    if (dangMo) ghi(goi(PINGREQ, 0, ByteArray(0)))
                }
            } catch (_: Exception) {
            }
        }
    }

    private fun docGoi(vao: DataInputStream): Triple<Int, Int, ByteArray> {
        val b0 = vao.readUnsignedByte()
        var n = 0
        var nhan = 1
        for (i in 0 until 4) {
            val b = vao.readUnsignedByte()
            n += (b and 0x7F) * nhan
            nhan *= 128
            if (b and 0x80 == 0) break
        }
        val than = ByteArray(n)
        vao.readFully(than)
        return Triple(b0 shr 4, b0 and 0x0F, than)
    }

    private fun vongDoc(vao: DataInputStream) {
        try {
            while (true) {
                val (loai, co, than) = docGoi(vao)
                if (loai == PUBLISH) {
                    val dai = ((than[0].toInt() and 0xFF) shl 8) or (than[1].toInt() and 0xFF)
                    val chuDe = String(than, 2, dai, Charsets.UTF_8)
                    var i = 2 + dai
                    if ((co shr 1) and 3 > 0) i += 2                   // mã gói của QoS 1 (không dùng)
                    khiNhan(chuDe, than.copyOfRange(i, than.size), co and 1 == 1)
                }
            }
        } catch (e: Exception) {
            if (dangMo) {
                dangMo = false
                khiMat(e.message ?: "mất kết nối MQTT")
            }
        }
    }

    private fun ghi(b: ByteArray) {
        synchronized(khoaGhi) {
            val r = ra ?: throw IOException("chưa kết nối")
            r.write(b)
            r.flush()
        }
    }

    fun dangKy(vararg boLoc: String) {
        maGoi = maGoi % 65535 + 1
        var than = byteArrayOf((maGoi shr 8).toByte(), maGoi.toByte())
        for (f in boLoc) than += chuoi(f) + byteArrayOf(0)
        ghi(goi(SUBSCRIBE, 2, than))
    }

    fun phat(chuDe: String, noiDung: ByteArray, retain: Boolean = false) {
        ghi(goi(PUBLISH, if (retain) 1 else 0, chuoi(chuDe) + noiDung))
    }

    /** dep = false: đóng TCP không gửi DISCONNECT (giả lập mất sóng) -> broker phát Last Will. */
    fun dong(dep: Boolean = true) {
        val dangMoTruoc = dangMo
        dangMo = false
        try {
            if (dep && dangMoTruoc) ghi(goi(DISCONNECT, 0, ByteArray(0)))
        } catch (_: IOException) {
        }
        try {
            socket?.close()
        } catch (_: IOException) {
        }
    }
}
