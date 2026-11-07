package vn.edu.ltm.goithoai

import android.annotation.SuppressLint
import android.content.Context
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.util.Random
import kotlin.math.roundToInt

/** Đọc các tuyến mẫu trong assets/tuyen_mau.json (cùng tệp với bản Python). */
fun docTuyenMau(ctx: Context): List<Tuyen> {
    val goc = JSONObject(ctx.assets.open("tuyen_mau.json").bufferedReader(Charsets.UTF_8).use { it.readText() })
    val ds = goc.getJSONArray("tuyen")
    return (0 until ds.length()).map { i ->
        val t = ds.getJSONObject(i)
        val d = t.getJSONArray("diem")
        Tuyen(t.getString("ten"), t.optString("diem_den", ""), t.optBoolean("vong", false),
            (0 until d.length()).map { j -> d.getJSONArray(j).let { doubleArrayOf(it.getDouble(0), it.getDouble(1)) } })
    }
}

/**
 * Bài mở rộng MR6.15 – phía TÀI XẾ: lấy vị trí (GPS thật hoặc giả lập theo tuyến) và PUBLISH qua MQTT.
 *   xe/<mã xe>/vitri      QoS 0, retain: {"lat","lon","toc_do","huong","do_chinh_xac","nguon","t","ten","xe",...}
 *   xe/<mã xe>/trangthai  "truc_tuyen" khi vào mạng, "nghi" khi tạm ẩn / thoát đúng cách,
 *                         "mat_ket_noi" = Last Will – broker tự phát khi tài xế mất sóng (TCP đứt, quá keepalive).
 * Các trường trạng thái là state của Compose: giao diện tự vẽ lại khi giá trị đổi. Hàm công khai gọi trên luồng chính.
 */
class TaiXeViTri(private val ctx: Context, private val scope: CoroutineScope) {
    val tuyenMau: List<Tuyen> = try {
        docTuyenMau(ctx)
    } catch (e: Exception) {
        emptyList()
    }
    var tuyenGpx by mutableStateOf<Tuyen?>(null)

    var cheDo by mutableStateOf("gia_lap")                 // "gps" | "gia_lap"
    var mqtt by mutableStateOf("chưa kết nối")
    var mqttOk by mutableStateOf(false)
    var anViTri by mutableStateOf(false)
    var tuyenChon by mutableIntStateOf(0)                  // chỉ số trong tuyenMau; -1 = tệp GPX
    var tocDo by mutableDoubleStateOf(30.0)                // km/h (giả lập)
    var chuKy by mutableIntStateOf(1)                      // giây giữa hai lần gửi
    var nhieu by mutableIntStateOf(5)                      // ± mét
    var dangGiaLap by mutableStateOf(false)
    var tamDung by mutableStateOf(false)
    var daChayM by mutableDoubleStateOf(0.0)
    var soDiem by mutableIntStateOf(0)
    var viTri by mutableStateOf<DoubleArray?>(null)
    var huongDo by mutableDoubleStateOf(0.0)
    var tocDoHienTai by mutableDoubleStateOf(0.0)
    var doChinhXac by mutableDoubleStateOf(0.0)
    var guiLanCuoi by mutableLongStateOf(0L)
    var gpsTrangThai by mutableStateOf("")
    val vet = mutableStateListOf<DoubleArray>()

    private var khach: MqttKhach? = null
    private var maXe = ""
    private var ten = ""
    private var xe = ""
    private var viecGiaLap: Job? = null
    private var gpsDangBat = false
    private val ngauNhien = Random()
    private val lm = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager

    val tuyen: Tuyen? get() = if (tuyenChon < 0) tuyenGpx else tuyenMau.getOrNull(tuyenChon)
    private val chuDeViTri get() = "xe/$maXe/vitri"
    private val chuDeTrangThai get() = "xe/$maXe/trangthai"

    fun ketNoi(host: String, cong: Int, maXe: String, ten: String, xe: String) {
        this.maXe = maXe; this.ten = ten; this.xe = xe
        if (khach?.dangMo == true) return
        mqtt = "đang kết nối MQTT $host:$cong…"
        scope.launch {
            val k = MqttKhach("taixe-$maXe", { _, _, _ -> }, { lyDo ->
                scope.launch { mqttOk = false; mqtt = "mất kết nối MQTT: $lyDo" }
            }, keepalive = 10)
            try {
                withContext(Dispatchers.IO) {
                    k.ketNoi(host, cong, Triple(chuDeTrangThai, "mat_ket_noi".toByteArray(), true))
                    k.phat(chuDeTrangThai, "truc_tuyen".toByteArray(), retain = true)
                }
                khach = k
                mqttOk = true
                anViTri = false
                mqtt = "MQTT $host:$cong · chủ đề $chuDeViTri"
            } catch (e: Exception) {
                mqttOk = false
                mqtt = "không kết nối được MQTT $host:$cong (${e.message})"
            }
        }
    }

    private fun phat(chuDe: String, noiDung: ByteArray, retain: Boolean) {
        val k = khach ?: return
        scope.launch(Dispatchers.IO) {
            try {
                k.phat(chuDe, noiDung, retain)
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { mqttOk = false; mqtt = "gửi MQTT lỗi: ${e.message}" }
            }
        }
    }

    /** Gửi một điểm vị trí (và cập nhật màn hình). */
    private fun guiDiem(p: DoubleArray, huong: Double, tocDoKmh: Double, chinhXac: Double, nguon: String,
                        tuyen: Tuyen?, conLaiM: Double?) {
        viTri = p
        huongDo = huong
        tocDoHienTai = tocDoKmh
        doChinhXac = chinhXac
        if (vet.size >= 300) vet.removeAt(0)
        vet.add(p)
        if (anViTri || !mqttOk) return
        val tin = JSONObject()
            .put("lat", Math.round(p[0] * 1e6) / 1e6).put("lon", Math.round(p[1] * 1e6) / 1e6)
            .put("toc_do", Math.round(tocDoKmh * 10) / 10.0).put("huong", huong.roundToInt())
            .put("do_chinh_xac", Math.round(chinhXac * 10) / 10.0).put("nguon", nguon)
            .put("t", System.currentTimeMillis()).put("ten", ten).put("xe", xe)
        if (tuyen != null) {
            tin.put("tuyen", tuyen.ten)
            if (!tuyen.vong) {
                val d = tuyen.diem.last()
                tin.put("dich", JSONObject().put("lat", d[0]).put("lon", d[1]).put("ten", tuyen.diemDen))
            }
        }
        if (conLaiM != null) tin.put("con_lai_m", conLaiM.roundToInt())
        phat(chuDeViTri, tin.toString().toByteArray(Charsets.UTF_8), retain = true)
        soDiem++
        guiLanCuoi = System.currentTimeMillis()
    }

    fun chonCheDo(c: String) {
        if (c == cheDo) return
        if (c == "gps") dungGiaLap() else tatGps()
        cheDo = c
    }

    // ------------------------------------------------------------------ GPS thật
    private val ngheGps = object : LocationListener {
        override fun onLocationChanged(l: Location) {
            gpsTrangThai = "GPS ± ${l.accuracy.roundToInt()} m"
            guiDiem(doubleArrayOf(l.latitude, l.longitude), if (l.hasBearing()) l.bearing.toDouble() else huongDo,
                if (l.hasSpeed()) l.speed * 3.6 else 0.0, l.accuracy.toDouble(), "gps", null, null)
        }

        @Deprecated("API cũ, vẫn cần cho Android < 10")
        override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
        override fun onProviderEnabled(provider: String) {
            gpsTrangThai = "GPS đã bật – đang chờ tín hiệu vệ tinh…"
        }
        override fun onProviderDisabled(provider: String) {
            gpsTrangThai = "GPS đang tắt – bật Vị trí trong Cài đặt"
        }
    }

    /** Bật nhận vị trí GPS thật mỗi chu kỳ (cần quyền ACCESS_FINE_LOCATION – MainActivity đã xin). */
    @SuppressLint("MissingPermission")
    fun batGps() {
        if (gpsDangBat) return
        try {
            if (!lm.isProviderEnabled(LocationManager.GPS_PROVIDER)) {
                gpsTrangThai = "GPS đang tắt – bật Vị trí trong Cài đặt"
            } else {
                gpsTrangThai = "đang chờ tín hiệu vệ tinh… (ra gần cửa sổ / ngoài trời)"
            }
            lm.requestLocationUpdates(LocationManager.GPS_PROVIDER, chuKy * 1000L, 0f, ngheGps, Looper.getMainLooper())
            gpsDangBat = true
        } catch (e: SecurityException) {
            gpsTrangThai = "chưa được cấp quyền vị trí"
        } catch (e: IllegalArgumentException) {
            gpsTrangThai = "máy không có GPS – dùng chế độ Giả lập"
        }
    }

    fun tatGps() {
        if (gpsDangBat) lm.removeUpdates(ngheGps)
        gpsDangBat = false
    }

    // ------------------------------------------------------------------ giả lập
    /** Chạy dọc tuyến: mỗi chu kỳ tiến thêm tốc_độ × chu_kỳ mét, thêm nhiễu GPS rồi gửi. */
    fun batGiaLap() {
        val t = tuyen ?: return
        if (dangGiaLap) {
            tamDung = false
            return
        }
        dangGiaLap = true
        tamDung = false
        viecGiaLap = scope.launch {
            while (isActive) {
                val (p, h) = t.tai(daChayM)
                val conLai = if (t.vong) null else (t.dai - daChayM).coerceAtLeast(0.0)
                val toi = conLai != null && conLai <= 0.0
                if (!tamDung) {
                    guiDiem(HinhHoc.themNhieu(p, nhieu.toDouble(), ngauNhien), h, if (toi) 0.0 else tocDo,
                        nhieu.toDouble(), "gia_lap", t, conLai)
                }
                if (toi) {
                    dangGiaLap = false
                    break
                }
                delay(chuKy * 1000L)
                if (!tamDung) daChayM += tocDo / 3.6 * chuKy
            }
        }
    }

    fun doiTamDung() {
        tamDung = !tamDung
    }

    fun dungGiaLap() {
        viecGiaLap?.cancel()
        viecGiaLap = null
        dangGiaLap = false
        tamDung = false
        daChayM = 0.0
    }

    /** Tạm ẩn: ngừng gửi, xóa điểm giữ lại (payload rỗng + retain) để người đăng ký sau không thấy vị trí cũ. */
    fun doiAnViTri() {
        anViTri = !anViTri
        if (anViTri) {
            phat(chuDeViTri, ByteArray(0), retain = true)
            phat(chuDeTrangThai, "nghi".toByteArray(), retain = true)
        } else {
            phat(chuDeTrangThai, "truc_tuyen".toByteArray(), retain = true)
        }
    }

    fun napGpx(ten: String, noiDung: String): String? {
        val diem = HinhHoc.docGpx(noiDung)
        if (diem.size < 2) return "tệp GPX không có đủ điểm (cần thẻ <trkpt lat=… lon=…>)"
        dungGiaLap()
        tuyenGpx = Tuyen(ten.removeSuffix(".gpx"), ten.removeSuffix(".gpx"), false, diem)
        tuyenChon = -1
        return null
    }

    /** Thoát đúng cách: báo "nghi" rồi DISCONNECT (broker KHÔNG phát Last Will). */
    fun dong() {
        dungGiaLap()
        tatGps()
        val k = khach ?: return
        khach = null
        mqttOk = false
        mqtt = "chưa kết nối"
        scope.launch(Dispatchers.IO) {
            try {
                k.phat(chuDeTrangThai, "nghi".toByteArray(), retain = true)
            } catch (_: Exception) {
            }
            k.dong(dep = true)
        }
    }
}

/** Một điểm vị trí tài xế nhận qua MQTT. */
data class ViTriXe(
    val lat: Double, val lon: Double, val tocDo: Double, val huong: Double, val doChinhXac: Double,
    val nguon: String, val t: Long, val ten: String, val xe: String, val tuyen: String,
    val conLaiM: Double?, val dich: DoubleArray?, val dichTen: String,
) {
    companion object {
        fun tu(j: JSONObject): ViTriXe {
            val d = j.optJSONObject("dich")
            return ViTriXe(
                j.getDouble("lat"), j.getDouble("lon"), j.optDouble("toc_do", 0.0), j.optDouble("huong", 0.0),
                j.optDouble("do_chinh_xac", 0.0), j.optString("nguon", "gps"), j.optLong("t", 0L),
                j.optString("ten", ""), j.optString("xe", ""), j.optString("tuyen", ""),
                if (j.has("con_lai_m")) j.optDouble("con_lai_m") else null,
                d?.let { doubleArrayOf(it.getDouble("lat"), it.getDouble("lon")) }, d?.optString("ten", "") ?: "",
            )
        }
    }
}

/**
 * Bài mở rộng MR6.15 – phía KHÁCH: SUBSCRIBE đúng hai chủ đề của xe mình đã đặt. Nhờ retain, ngay sau SUBACK broker
 * gửi điểm cuối cùng nên bản đồ có xe ngay, không phải chờ chu kỳ gửi tiếp theo.
 */
class KhachTheoDoi(private val scope: CoroutineScope) {
    var maXe by mutableStateOf("")
    var mqtt by mutableStateOf("")
    var viTri by mutableStateOf<ViTriXe?>(null)
    var nhanLuc by mutableLongStateOf(0L)
    var trangThai by mutableStateOf("")
    var anViTri by mutableStateOf(false)
    val vet = mutableStateListOf<DoubleArray>()
    private var khach: MqttKhach? = null

    fun batDau(host: String, cong: Int, maKhach: String, maXe: String) {
        dung()
        this.maXe = maXe
        viTri = null
        nhanLuc = 0L
        trangThai = ""
        anViTri = false
        vet.clear()
        mqtt = "đang kết nối MQTT $host:$cong…"
        val k = MqttKhach("khach-$maKhach-${System.currentTimeMillis() % 100000}", { chuDe, nd, _ ->
            scope.launch(Dispatchers.Main) { nhan(chuDe, nd) }
        }, { lyDo -> scope.launch(Dispatchers.Main) { mqtt = "mất kết nối MQTT: $lyDo" } }, keepalive = 15)
        khach = k
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    k.ketNoi(host, cong)
                    k.dangKy("xe/$maXe/vitri", "xe/$maXe/trangthai")
                }
                mqtt = "đăng ký xe/$maXe/vitri"
            } catch (e: Exception) {
                mqtt = "không kết nối được MQTT $host:$cong (${e.message})"
            }
        }
    }

    private fun nhan(chuDe: String, nd: ByteArray) {
        if (chuDe.endsWith("/trangthai")) {
            trangThai = String(nd, Charsets.UTF_8)
            return
        }
        if (nd.isEmpty()) {                                  // tài xế tạm ẩn vị trí (xóa tin giữ lại)
            anViTri = true
            viTri = null
            return
        }
        try {
            val v = ViTriXe.tu(JSONObject(String(nd, Charsets.UTF_8)))
            anViTri = false
            viTri = v
            nhanLuc = System.currentTimeMillis()
            if (vet.size >= 300) vet.removeAt(0)
            vet.add(doubleArrayOf(v.lat, v.lon))
        } catch (_: Exception) {
        }
    }

    fun dung() {
        val k = khach ?: return
        khach = null
        scope.launch(Dispatchers.IO) { k.dong() }
    }
}
