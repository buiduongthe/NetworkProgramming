package vn.edu.ltm.goithoai

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

/**
 * Bài mở rộng MR6.15 – kênh báo hiệu tới máy chủ trung gian: WebSocket ws://<IP>:<cổng>/ws (hoặc wss:// khi máy chủ
 * chạy --tls), mỗi tin một đối tượng JSON (giao thức ở đầu may_chu.py / maychu.go). Cùng một OkHttpClient dùng để tải
 * bản ghi âm GET /banghi/<tệp>.
 *
 * TLS trong lab: máy chủ dùng chứng chỉ tự ký nên không có CA nào xác nhận được. Thay vào đó app "ghim" vân tay
 * SHA-256 của chứng chỉ (certificate pinning): nếu người dùng nhập vân tay (in ra ở cửa sổ máy chủ) thì phải khớp đúng;
 * nếu để trống thì tin lần đầu (TOFU) – app hiện vân tay để so với máy chủ và lưu lại, lần sau khác vân tay sẽ từ chối.
 * Mọi callback chạy trên luồng của OkHttp, KHÔNG phải luồng giao diện.
 */
class BaoHieu(private val nghe: NguoiNghe) {
    interface NguoiNghe {
        fun khiMo(vanTay: String?)
        fun khiNhan(tin: JSONObject)
        fun khiDong(lyDo: String, batThuong: Boolean)
    }

    private var ws: WebSocket? = null
    private var client: OkHttpClient? = null
    private var goc = ""                          // http(s)://host:cổng
    @Volatile private var chuDongDong = false

    /** Vân tay SHA-256 của chứng chỉ máy chủ vừa thấy (chỉ khi dùng TLS). */
    @Volatile var vanTayThay: String? = null
        private set

    companion object {
        fun vanTay(cert: X509Certificate): String =
            MessageDigest.getInstance("SHA-256").digest(cert.encoded).joinToString(":") { "%02X".format(it) }

        fun chuanHoaVanTay(s: String): String =
            s.uppercase().filter { it in "0123456789ABCDEF" }.chunked(2).joinToString(":")
    }

    fun ketNoi(host: String, cong: Int, tls: Boolean, vanTayMongDoi: String) {
        chuDongDong = false
        val b = OkHttpClient.Builder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .readTimeout(0, TimeUnit.SECONDS)               // WebSocket: không giới hạn thời gian đọc
            .pingInterval(15, TimeUnit.SECONDS)             // ping/pong của WebSocket để phát hiện mất mạng
        if (tls) {
            val mongDoi = chuanHoaVanTay(vanTayMongDoi)
            val tm = object : X509TrustManager {
                override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {}
                override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                    val c = chain?.firstOrNull() ?: throw CertificateException("máy chủ không gửi chứng chỉ")
                    val vt = vanTay(c)
                    vanTayThay = vt
                    if (mongDoi.isNotEmpty() && mongDoi != vt)
                        throw CertificateException("SAI vân tay chứng chỉ máy chủ (có thể bị giả mạo): $vt")
                }
                override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
            }
            val ctx = SSLContext.getInstance("TLS")
            ctx.init(null, arrayOf(tm), SecureRandom())
            // Đã kiểm bằng vân tay nên bỏ kiểm tên máy (chứng chỉ lab cấp cho IP thay đổi theo phòng máy).
            b.sslSocketFactory(ctx.socketFactory, tm).hostnameVerifier { _, _ -> true }
        }
        val c = b.build()
        client = c
        goc = "${if (tls) "https" else "http"}://$host:$cong"
        val url = "${if (tls) "wss" else "ws"}://$host:$cong/ws"
        ws = c.newWebSocket(Request.Builder().url(url).build(), object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                nghe.khiMo(vanTayThay)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                val tin = try {
                    JSONObject(text)
                } catch (e: Exception) {
                    return
                }
                nghe.khiNhan(tin)
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(1000, null)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (ws === webSocket) nghe.khiDong(if (reason.isNotEmpty()) reason else "máy chủ đóng kết nối", !chuDongDong)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (ws === webSocket) nghe.khiDong(moTaLoi(t), !chuDongDong)
            }
        })
    }

    private fun moTaLoi(t: Throwable): String {
        var e: Throwable? = t
        while (e != null) {                                    // lỗi vân tay nằm sâu trong chuỗi nguyên nhân
            if (e is CertificateException) return e.message ?: "lỗi chứng chỉ"
            e = e.cause
        }
        return when (t) {
            is java.net.ConnectException -> "không kết nối được máy chủ (sai IP/cổng hoặc tường lửa chặn)"
            is java.net.SocketTimeoutException -> "hết thời gian chờ máy chủ"
            is java.net.UnknownHostException -> "không tìm thấy máy chủ"
            else -> t.message ?: t.javaClass.simpleName
        }
    }

    fun gui(tin: JSONObject): Boolean = ws?.send(tin.toString()) ?: false

    fun gui(loai: String, vararg truong: Pair<String, Any?>): Boolean {
        val tin = JSONObject().put("loai", loai)
        for ((k, v) in truong) tin.put(k, v ?: JSONObject.NULL)
        return gui(tin)
    }

    fun dong() {
        chuDongDong = true
        ws?.close(1000, "tạm biệt")
        ws = null
    }

    /** Đường dẫn công khai của một bản ghi (để mở bằng trình duyệt khi máy không phát được). */
    fun urlBanGhi(ten: String) = "$goc/banghi/$ten"

    /** Tải bản ghi về tệp (chặn – gọi trên luồng nền). Dùng lại tệp đã tải nếu có. */
    fun taiBanGhi(ten: String, dich: File): File {
        if (dich.exists() && dich.length() > 0) return dich
        val c = client ?: throw IOException("chưa kết nối máy chủ")
        c.newCall(Request.Builder().url(urlBanGhi(ten)).build()).execute().use { r ->
            if (!r.isSuccessful) throw IOException("máy chủ trả HTTP ${r.code}")
            dich.parentFile?.mkdirs()
            val tam = File(dich.path + ".tam")
            tam.outputStream().use { ra -> r.body!!.byteStream().copyTo(ra) }
            if (!tam.renameTo(dich)) throw IOException("không ghi được tệp")
        }
        return dich
    }
}
