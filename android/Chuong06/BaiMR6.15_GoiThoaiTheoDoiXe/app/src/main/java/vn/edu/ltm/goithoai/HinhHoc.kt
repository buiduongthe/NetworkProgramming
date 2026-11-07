package vn.edu.ltm.goithoai

import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Bài mở rộng MR6.15 – tính toán vị trí (thuần Kotlin, không phụ thuộc Android; cùng công thức với tai_xe_gia.py).
 * Điểm = DoubleArray(lat, lon) tính bằng độ.
 */
object HinhHoc {
    const val R_TRAI_DAT = 6371000.0

    /** Khoảng cách haversine (m) giữa hai điểm. */
    fun khoangCach(a: DoubleArray, b: DoubleArray): Double {
        val la1 = Math.toRadians(a[0]); val la2 = Math.toRadians(b[0])
        val dLa = la2 - la1; val dLo = Math.toRadians(b[1] - a[1])
        val h = sin(dLa / 2) * sin(dLa / 2) + cos(la1) * cos(la2) * sin(dLo / 2) * sin(dLo / 2)
        return 2 * R_TRAI_DAT * asin(sqrt(min(1.0, h)))
    }

    /** Hướng đi (độ, 0 = bắc, theo chiều kim đồng hồ) từ a tới b. */
    fun huong(a: DoubleArray, b: DoubleArray): Double {
        val la1 = Math.toRadians(a[0]); val la2 = Math.toRadians(b[0]); val dl = Math.toRadians(b[1] - a[1])
        val y = sin(dl) * cos(la2)
        val x = cos(la1) * sin(la2) - sin(la1) * cos(la2) * cos(dl)
        return (Math.toDegrees(atan2(y, x)) + 360) % 360
    }

    /** Dịch điểm ngẫu nhiên: độ lệch chuẩn m/2 mét mỗi trục (≈ 95 % điểm nằm trong ± m). */
    fun themNhieu(p: DoubleArray, m: Double, ngauNhien: java.util.Random): DoubleArray {
        if (m <= 0) return p
        val dy = ngauNhien.nextGaussian() * m / 2
        val dx = ngauNhien.nextGaussian() * m / 2
        return doubleArrayOf(p[0] + dy / 111320.0, p[1] + dx / (111320.0 * cos(Math.toRadians(p[0]))))
    }

    /** Thời gian tới nơi (phút, làm tròn lên, tối thiểu 1) với quãng đường m mét; tốc độ dưới 15 km/h coi như 15. */
    fun uocPhut(m: Double, tocDoKmh: Double): Int {
        val v = max(tocDoKmh, 15.0) / 3.6
        return max(1, kotlin.math.ceil(m / v / 60).toInt())
    }

    /** Đọc các điểm <trkpt>/<rtept>/<wpt lat=".." lon=".."> của tệp GPX (đủ cho tệp xuất từ ứng dụng ghi GPS). */
    fun docGpx(noiDung: String): List<DoubleArray> {
        val the = Regex("""<(?:trkpt|rtept|wpt)\b([^>]*)>""")
        val lat = Regex("""\blat\s*=\s*["']([-0-9.]+)["']""")
        val lon = Regex("""\blon\s*=\s*["']([-0-9.]+)["']""")
        val kq = ArrayList<DoubleArray>()
        for (m in the.findAll(noiDung)) {
            val a = lat.find(m.groupValues[1])?.groupValues?.get(1)?.toDoubleOrNull() ?: continue
            val o = lon.find(m.groupValues[1])?.groupValues?.get(1)?.toDoubleOrNull() ?: continue
            if (a in -90.0..90.0 && o in -180.0..180.0) kq.add(doubleArrayOf(a, o))
        }
        return kq
    }
}

/** Một tuyến giả lập: danh sách điểm; vong = true thì chạy vòng lặp (không có điểm đến). */
class Tuyen(val ten: String, val diemDen: String, val vong: Boolean, val diem: List<DoubleArray>) {
    private val doan = (0 until diem.size - 1).map { HinhHoc.khoangCach(diem[it], diem[it + 1]) }
    val dai: Double = doan.sum()

    init {
        require(diem.size >= 2) { "tuyến cần ít nhất 2 điểm" }
    }

    /** Điểm và hướng tại quãng đường s (m) tính từ đầu tuyến. */
    fun tai(s0: Double): Pair<DoubleArray, Double> {
        var s = if (vong && dai > 0) ((s0 % dai) + dai) % dai else s0.coerceIn(0.0, dai)
        for (i in doan.indices) {
            val d = doan[i]
            if (s <= d || i == doan.size - 1) {
                val k = if (d > 0) min(1.0, s / d) else 0.0
                val a = diem[i]; val b = diem[i + 1]
                return Pair(doubleArrayOf(a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k), HinhHoc.huong(a, b))
            }
            s -= d
        }
        return Pair(diem.last(), 0.0)
    }
}
