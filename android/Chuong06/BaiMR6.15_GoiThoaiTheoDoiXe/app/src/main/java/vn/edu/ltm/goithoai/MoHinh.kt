package vn.edu.ltm.goithoai

import android.content.Context
import org.json.JSONObject

/** Bài mở rộng MR6.15 – các kiểu dữ liệu dùng chung giữa báo hiệu và giao diện. */

/** Thiết lập kết nối, lưu lại trong SharedPreferences cho lần mở app sau. */
data class CaiDat(
    val host: String = "192.168.1.10",
    val cong: String = "8080",
    val congMqtt: String = "1883",
    val ten: String = "",
    val ma: String = "",
    val vaiTro: String = "khach",                 // "khach" | "tai_xe"
    val dongYGhiAm: Boolean = true,
    val tls: Boolean = false,
    val vanTay: String = "",                       // SHA-256 chứng chỉ máy chủ (trống = tin lần đầu và lưu lại)
    val bienSo: String = "",
    val xe: String = "",
    val chuyen: String = "",
    val diemDon: String = "",
) {
    fun luu(ctx: Context) {
        ctx.getSharedPreferences("cai_dat", Context.MODE_PRIVATE).edit()
            .putString("host", host).putString("cong", cong).putString("cong_mqtt", congMqtt)
            .putString("ten", ten).putString("ma", ma).putString("vai_tro", vaiTro)
            .putBoolean("dong_y_ghi_am", dongYGhiAm).putBoolean("tls", tls).putString("van_tay", vanTay)
            .putString("bien_so", bienSo).putString("xe", xe).putString("chuyen", chuyen).putString("diem_don", diemDon)
            .apply()
    }

    companion object {
        fun doc(ctx: Context): CaiDat {
            val p = ctx.getSharedPreferences("cai_dat", Context.MODE_PRIVATE)
            val m = CaiDat()
            return CaiDat(
                p.getString("host", m.host)!!, p.getString("cong", m.cong)!!, p.getString("cong_mqtt", m.congMqtt)!!,
                p.getString("ten", m.ten)!!, p.getString("ma", m.ma)!!, p.getString("vai_tro", m.vaiTro)!!,
                p.getBoolean("dong_y_ghi_am", m.dongYGhiAm), p.getBoolean("tls", m.tls), p.getString("van_tay", m.vanTay)!!,
                p.getString("bien_so", m.bienSo)!!, p.getString("xe", m.xe)!!, p.getString("chuyen", m.chuyen)!!,
                p.getString("diem_don", m.diemDon)!!,
            )
        }
    }
}

/** Một người trong danh bạ (máy chủ gửi qua tin "danh_ba"). */
data class LienHe(
    val ma: String, val ten: String, val vaiTro: String, val moTa: String, val maXe: String,
    val trucTuyen: Boolean, val ban: Boolean,
) {
    val laTaiXe get() = vaiTro == "tai_xe"

    companion object {
        fun tu(j: JSONObject?) = LienHe(
            j?.optString("ma", "") ?: "", j?.optString("ten", "?") ?: "?", j?.optString("vai_tro", "khach") ?: "khach",
            j?.optString("mo_ta", "") ?: "", j?.optString("ma_xe", "") ?: "",
            j?.optBoolean("truc_tuyen", false) ?: false, j?.optBoolean("ban", false) ?: false,
        )
    }
}

enum class TrangThaiGoi { DANG_GOI, DO_CHUONG, CUOC_DEN, DANG_KET_NOI, DANG_NOI }

/** Cuộc gọi đang diễn ra (chỉ một cuộc tại một thời điểm). */
data class CuocGoi(
    val maCuoc: String?,
    val doiPhuong: LienHe,
    val goiDi: Boolean,
    val trangThai: TrangThaiGoi,
    val ghiAm: Boolean = false,
    val batDau: Long = 0L,                          // lúc bắt đầu nói (ms) để đếm thời lượng
    val tatMic: Boolean = false,
    val loaNgoai: Boolean = false,
    val giuMay: Boolean = false,
    val doiPhuongGiu: Boolean = false,
    val doiPhuongTatMic: Boolean = false,
    val rttMs: Double? = null,
    val matGoi: Double? = null,
    val kbps: Double? = null,
    val webRtc: String = "",
)

/** Một dòng lịch sử cuộc gọi (máy chủ gửi qua tin "lich_su"). banGhi: mã người -> tệp, "tron" -> tệp trộn. */
data class MucLichSu(
    val maCuoc: String, val maGoi: String, val tenGoi: String, val maNghe: String, val tenNghe: String,
    val batDau: String, val thoiLuong: Double, val ketQua: String, val lyDo: String,
    val banGhi: Map<String, String>, val kichThuoc: Long,
) {
    companion object {
        fun tu(j: JSONObject): MucLichSu {
            val g = j.optJSONObject("goi")
            val n = j.optJSONObject("nghe")
            val b = j.optJSONObject("ban_ghi")
            val bg = LinkedHashMap<String, String>()
            b?.keys()?.forEach { k -> bg[k] = b.optString(k) }
            return MucLichSu(
                j.optString("ma_cuoc"), g?.optString("ma", "") ?: "", g?.optString("ten", "?") ?: "?",
                n?.optString("ma", "") ?: "", n?.optString("ten", "?") ?: "?", j.optString("bat_dau"),
                j.optDouble("thoi_luong", 0.0), j.optString("ket_qua"), j.optString("ly_do"), bg,
                j.optLong("kich_thuoc", 0L),
            )
        }
    }
}

/** Bản ghi đang mở trong trình phát của màn hình Lịch sử. */
data class TrinhPhat(
    val maCuoc: String,
    val kenh: String,                               // "tron" hoặc mã người
    val tep: String,
    val dangPhat: Boolean = false,
    val viTriMs: Int = 0,
    val tongMs: Int = 0,
    val dangTai: Boolean = false,
    val loi: String? = null,
)

fun mmss(giay: Double): String {
    val s = giay.toLong().coerceAtLeast(0)
    return "%02d:%02d".format(s / 60, s % 60)
}

fun kichThuocDep(b: Long): String = when {
    b >= 1_000_000 -> "%.1f MB".format(b / 1_000_000.0).replace('.', ',')
    b >= 1000 -> "${b / 1000} kB"
    else -> "$b B"
}

fun soVn(x: Double, chuSo: Int = 1): String = "%.${chuSo}f".format(x).replace('.', ',')

fun moTaKetQua(k: String): String = when (k) {
    "hoan_thanh" -> "Đã nói"
    "nho" -> "Gọi nhỡ"
    "tu_choi" -> "Từ chối"
    "huy" -> "Đã hủy"
    "ban" -> "Máy bận"
    "mat_ket_noi" -> "Mất kết nối"
    "may_chu_ngat" -> "Máy chủ ngắt"
    else -> k
}
