package vn.edu.ltm.goithoai

import android.app.Application
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.media.ToneGenerator
import android.media.Ringtone
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

/**
 * Bài mở rộng MR6.15 – trạng thái của cả ứng dụng và xử lý giao thức báo hiệu.
 * Mọi thay đổi state chạy trên luồng chính (tin WebSocket tới trên luồng OkHttp được chuyển về bằng viewModelScope).
 */
class UngDungViewModel(app: Application) : AndroidViewModel(app) {
    private val ctx = app.applicationContext

    var caiDat by mutableStateOf(CaiDat.doc(ctx))
        private set
    var ketNoi by mutableStateOf("chua")                    // "chua" | "dang" | "da"
        private set
    var loiKetNoi by mutableStateOf<String?>(null)
    var maXeCuaToi by mutableStateOf("")
        private set
    var mayChu by mutableStateOf("")
        private set
    var vanTayMayChu by mutableStateOf<String?>(null)
        private set
    var danhBa by mutableStateOf<List<LienHe>>(emptyList())
        private set
    var cuoc by mutableStateOf<CuocGoi?>(null)
        private set
    var lichSu by mutableStateOf<List<MucLichSu>>(emptyList())
        private set
    var thongBao by mutableStateOf<String?>(null)
    var tab by mutableIntStateOf(0)                         // 0 danh bạ, 1 lịch sử, 2 vị trí/chuyến đi
    var trinhPhat by mutableStateOf<TrinhPhat?>(null)
        private set

    val taiXe = TaiXeViTri(ctx, viewModelScope)
    val theoDoi = KhachTheoDoi(viewModelScope)

    private val baoHieu = BaoHieu(object : BaoHieu.NguoiNghe {
        override fun khiMo(vanTay: String?) {
            viewModelScope.launch { daMo(vanTay) }
        }

        override fun khiNhan(tin: JSONObject) {
            viewModelScope.launch { xuLy(tin) }
        }

        override fun khiDong(lyDo: String, batThuong: Boolean) {
            viewModelScope.launch { daDong(lyDo, batThuong) }
        }
    })
    private var amThanh: AmThanhWebRtc? = null
    private var chuong: Ringtone? = null
    private var hoiChuong: ToneGenerator? = null
    private var mp: MediaPlayer? = null
    private var viecPhat: Job? = null

    fun capNhatCaiDat(c: CaiDat) {
        caiDat = c
    }

    // ================================================================== kết nối
    fun ketNoiMayChu() {
        val c = caiDat.copy(host = caiDat.host.trim(), ma = caiDat.ma.trim(), ten = caiDat.ten.trim())
        val cong = c.cong.toIntOrNull()
        loiKetNoi = when {
            c.host.isEmpty() -> "Nhập địa chỉ IP máy chủ"
            cong == null || cong !in 1..65535 -> "Cổng phải là số 1–65535"
            c.ma.isEmpty() || c.ma.length > 20 -> "Mã người dùng 1–20 ký tự"
            c.ten.isEmpty() -> "Nhập tên hiển thị"
            else -> null
        }
        if (loiKetNoi != null || cong == null) return
        caiDat = c
        c.luu(ctx)
        ketNoi = "dang"
        baoHieu.ketNoi(c.host, cong, c.tls, c.vanTay)
    }

    private fun daMo(vanTay: String?) {
        vanTayMayChu = vanTay
        if (vanTay != null && caiDat.vanTay.isBlank()) {          // tin lần đầu: lưu vân tay cho lần sau
            caiDat = caiDat.copy(vanTay = vanTay)
            caiDat.luu(ctx)
        }
        val c = caiDat
        baoHieu.gui("dang_nhap", "ma" to c.ma, "ten" to c.ten, "vai_tro" to c.vaiTro, "bien_so" to c.bienSo,
            "xe" to c.xe, "chuyen" to c.chuyen, "diem_don" to c.diemDon, "dong_y_ghi_am" to c.dongYGhiAm)
    }

    private fun daDong(lyDo: String, batThuong: Boolean) {
        val dangKetNoi = ketNoi == "dang"
        ketThucAmThanh()
        cuoc = null
        ketNoi = "chua"
        danhBa = emptyList()
        taiXe.dong()
        theoDoi.dung()
        theoDoi.maXe = ""
        dungPhat()
        if (batThuong) loiKetNoi = if (dangKetNoi) lyDo else "Mất kết nối máy chủ: $lyDo"
    }

    fun ngatKetNoi() {
        baoHieu.dong()
        daDong("", false)
    }

    // ================================================================== tin từ máy chủ
    private fun xuLy(t: JSONObject) {
        val c = cuoc
        val maCuoc = t.optString("ma_cuoc", "")
        when (t.optString("loai")) {
            "dang_nhap_ok" -> {
                ketNoi = "da"
                loiKetNoi = null
                maXeCuaToi = t.optString("ma_xe", caiDat.ma)
                mayChu = t.optString("may_chu", "")
                tab = 0
                baoHieu.gui("lich_su")
                if (caiDat.vaiTro == "tai_xe")
                    taiXe.ketNoi(caiDat.host, caiDat.congMqtt.toIntOrNull() ?: 1883, maXeCuaToi, caiDat.ten, xeCuaToi())
            }
            "danh_ba" -> {
                val ds = t.optJSONArray("ds")
                danhBa = if (ds == null) emptyList() else (0 until ds.length()).map { LienHe.tu(ds.optJSONObject(it)) }
                    .filter { it.ma != caiDat.ma }
                    .sortedWith(compareByDescending<LienHe> { it.trucTuyen }.thenBy { it.ten.lowercase() })
                cuoc?.let { cg ->                                   // cập nhật mô tả bên kia nếu đổi
                    danhBa.firstOrNull { it.ma == cg.doiPhuong.ma }?.let { cuoc = cg.copy(doiPhuong = it) }
                }
            }
            "do_chuong" -> {
                val den = LienHe.tu(t.optJSONObject("den"))
                cuoc = (c ?: CuocGoi(null, den, true, TrangThaiGoi.DANG_GOI))
                    .copy(maCuoc = maCuoc, doiPhuong = den, trangThai = TrangThaiGoi.DO_CHUONG)
                batHoiChuong()
            }
            "cuoc_goi_den" -> {
                if (c != null) {                                     // đang bận cuộc khác (hiếm: máy chủ đã chặn)
                    baoHieu.gui("tu_choi", "ma_cuoc" to maCuoc, "ly_do" to "Máy bận")
                    return
                }
                cuoc = CuocGoi(maCuoc, LienHe.tu(t.optJSONObject("tu")), false, TrangThaiGoi.CUOC_DEN,
                    ghiAm = t.optBoolean("ghi_am"))
                batChuong()
            }
            "da_tra_loi" -> {
                if (c == null || c.maCuoc != maCuoc) return
                tatChuong()
                cuoc = c.copy(trangThai = TrangThaiGoi.DANG_KET_NOI, ghiAm = t.optBoolean("ghi_am"),
                    batDau = System.currentTimeMillis())
                batDauAmThanh(maCuoc)
            }
            "sdp" -> if (c != null && c.maCuoc == maCuoc) t.optJSONObject("sdp")?.let { amThanh?.nhanSdp(it) }
            "doi_phuong" -> if (c != null && c.maCuoc == maCuoc) {
                cuoc = c.copy(
                    doiPhuongGiu = if (t.has("giu_may")) t.optBoolean("giu_may") else c.doiPhuongGiu,
                    doiPhuongTatMic = if (t.has("tat_mic")) t.optBoolean("tat_mic") else c.doiPhuongTatMic,
                )
            }
            "da_ket_thuc" -> {
                if (c == null || (c.maCuoc != null && c.maCuoc != maCuoc)) return
                tatChuong()
                ketThucAmThanh()
                cuoc = null
                val ly = t.optString("ly_do")
                thongBao = "${moTaKetQua(t.optString("ket_qua"))}: $ly" +
                    (if (t.optDouble("thoi_luong", 0.0) > 0) " · ${mmss(t.optDouble("thoi_luong"))}" else "")
                baoHieu.gui("lich_su")
            }
            "lich_su" -> {
                val ds = t.optJSONArray("ds")
                lichSu = if (ds == null) emptyList() else (0 until ds.length()).mapNotNull { ds.optJSONObject(it) }
                    .map { MucLichSu.tu(it) }
            }
            "loi" -> {
                val ly = t.optString("ly_do", "lỗi")
                if (ketNoi == "dang") {
                    loiKetNoi = ly
                    baoHieu.dong()
                    ketNoi = "chua"
                    return
                }
                if (c != null && c.maCuoc == null) {                  // lời gọi không thành (người nhận ngoại tuyến…)
                    tatChuong()
                    cuoc = null
                }
                thongBao = ly
            }
        }
    }

    private fun xeCuaToi(): String = listOf(caiDat.xe, caiDat.bienSo).filter { it.isNotBlank() }.joinToString(" · ")

    // ================================================================== thao tác cuộc gọi
    fun goi(lh: LienHe) {
        if (cuoc != null) return
        if (!lh.trucTuyen) {
            thongBao = "${lh.ten} đang ngoại tuyến"
            return
        }
        cuoc = CuocGoi(null, lh, true, TrangThaiGoi.DANG_GOI)
        baoHieu.gui("goi", "den" to lh.ma)
    }

    /** Gọi tài xế đang theo dõi (tìm trong danh bạ theo mã xe). */
    fun goiTaiXeTheoDoi() {
        val lh = danhBa.firstOrNull { it.laTaiXe && (it.maXe == theoDoi.maXe || it.ma == theoDoi.maXe) }
        if (lh == null) thongBao = "Tài xế không có trong danh bạ trực tuyến" else goi(lh)
    }

    /** Tài xế bấm "Gọi khách": người khách gần nhất trong lịch sử còn trực tuyến, nếu không thì khách trực tuyến đầu tiên. */
    fun goiKhach() {
        val khach = danhBa.filter { !it.laTaiXe && it.trucTuyen }
        val ganNhat = lichSu.asSequence().map { if (it.maGoi == caiDat.ma) it.maNghe else it.maGoi }
            .mapNotNull { ma -> khach.firstOrNull { it.ma == ma } }.firstOrNull()
        val lh = ganNhat ?: khach.firstOrNull()
        if (lh == null) thongBao = "Không có khách nào đang trực tuyến" else goi(lh)
    }

    fun traLoi() {
        val c = cuoc ?: return
        if (c.trangThai != TrangThaiGoi.CUOC_DEN) return
        tatChuong()
        baoHieu.gui("tra_loi", "ma_cuoc" to c.maCuoc)
    }

    fun tuChoi(lyDo: String = "Từ chối") {
        val c = cuoc ?: return
        tatChuong()
        baoHieu.gui("tu_choi", "ma_cuoc" to c.maCuoc, "ly_do" to lyDo)
    }

    /** Hủy khi đang đổ chuông, kết thúc khi đang nói (máy chủ tự phân biệt theo trạng thái). */
    fun cupMay() {
        val c = cuoc ?: return
        if (c.maCuoc == null) {                                       // chưa có mã cuộc: chỉ đóng màn hình
            tatChuong()
            cuoc = null
            return
        }
        baoHieu.gui(if (c.trangThai == TrangThaiGoi.DO_CHUONG) "huy" else "ket_thuc", "ma_cuoc" to c.maCuoc)
    }

    fun doiTatMic() {
        val c = cuoc ?: return
        val bat = !c.tatMic
        cuoc = c.copy(tatMic = bat)
        amThanh?.datTatMic(bat)
        if (c.maCuoc != null) baoHieu.gui("tat_mic", "ma_cuoc" to c.maCuoc, "bat" to bat)
    }

    fun doiLoaNgoai() {
        val c = cuoc ?: return
        val bat = !c.loaNgoai
        cuoc = c.copy(loaNgoai = bat)
        amThanh?.datLoaNgoai(bat)
    }

    fun doiGiuMay() {
        val c = cuoc ?: return
        val bat = !c.giuMay
        cuoc = c.copy(giuMay = bat)
        amThanh?.datGiuMay(bat)
        if (c.maCuoc != null) baoHieu.gui("giu_may", "ma_cuoc" to c.maCuoc, "bat" to bat)
    }

    private fun batDauAmThanh(maCuoc: String) {
        ketThucAmThanh()
        val a = AmThanhWebRtc(ctx,
            guiTin = { baoHieu.gui(it) },
            khiTrangThai = cb@{ s ->
                val c = cuoc ?: return@cb
                cuoc = if (s == "đã kết nối" && c.trangThai == TrangThaiGoi.DANG_KET_NOI)
                    c.copy(trangThai = TrangThaiGoi.DANG_NOI, webRtc = s) else c.copy(webRtc = s)
            },
            khiThongKe = { rtt, mat, kbps ->
                cuoc?.let { cuoc = it.copy(rttMs = rtt, matGoi = mat, kbps = kbps ?: it.kbps) }
            })
        amThanh = a
        a.batDau(maCuoc)
        val c = cuoc ?: return
        if (c.tatMic) a.datTatMic(true)
        if (c.loaNgoai) a.datLoaNgoai(true)
    }

    private fun ketThucAmThanh() {
        amThanh?.dong()
        amThanh = null
    }

    // ------------------------------------------------------------------ chuông
    private fun batChuong() {
        tatChuong()
        try {
            val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            chuong = RingtoneManager.getRingtone(ctx, uri)?.also { it.play() }
        } catch (_: Exception) {
        }
    }

    /** Hồi chuông phía người gọi: âm "tút… tút…" chuẩn của tổng đài, phát trên luồng âm đàm thoại. */
    private fun batHoiChuong() {
        if (hoiChuong != null) return
        try {
            hoiChuong = ToneGenerator(AudioManager.STREAM_VOICE_CALL, 70).also {
                it.startTone(ToneGenerator.TONE_SUP_RINGTONE)
            }
        } catch (_: RuntimeException) {
        }
    }

    private fun tatChuong() {
        chuong?.stop()
        chuong = null
        hoiChuong?.stopTone()
        hoiChuong?.release()
        hoiChuong = null
    }

    // ================================================================== lịch sử và trình phát
    fun taiLichSu() {
        baoHieu.gui("lich_su")
    }

    /** Chọn kênh "tron" (cả hai), mã của tôi (chỉ tôi) hoặc mã bên kia rồi phát. */
    fun phat(m: MucLichSu, kenh: String) {
        val tep = m.banGhi[kenh] ?: return
        dungPhat()
        trinhPhat = TrinhPhat(m.maCuoc, kenh, tep, dangTai = true)
        viewModelScope.launch {
            try {
                val f = withContext(Dispatchers.IO) { baoHieu.taiBanGhi(tep, File(ctx.cacheDir, "ban_ghi/$tep")) }
                val p = MediaPlayer()
                p.setDataSource(f.path)
                p.prepare()
                p.setOnCompletionListener {
                    trinhPhat = trinhPhat?.copy(dangPhat = false, viTriMs = trinhPhat?.tongMs ?: 0)
                    viecPhat?.cancel()
                }
                p.start()
                mp = p
                trinhPhat = TrinhPhat(m.maCuoc, kenh, tep, dangPhat = true, tongMs = p.duration)
                viecPhat = launch {
                    while (isActive) {
                        mp?.let { if (it.isPlaying) trinhPhat = trinhPhat?.copy(viTriMs = it.currentPosition) }
                        delay(200)
                    }
                }
            } catch (e: Exception) {
                trinhPhat = TrinhPhat(m.maCuoc, kenh, tep,
                    loi = "Không phát được (${e.message ?: e.javaClass.simpleName}) – thử \"Mở bằng ứng dụng khác\"")
            }
        }
    }

    fun doiPhatTamDung() {
        val p = mp ?: return
        val t = trinhPhat ?: return
        if (p.isPlaying) {
            p.pause()
            trinhPhat = t.copy(dangPhat = false)
        } else {
            if (t.viTriMs >= t.tongMs) p.seekTo(0)
            p.start()
            trinhPhat = t.copy(dangPhat = true)
        }
    }

    fun tua(phan: Float) {
        val p = mp ?: return
        val t = trinhPhat ?: return
        val ms = (t.tongMs * phan).toInt()
        p.seekTo(ms)
        trinhPhat = t.copy(viTriMs = ms)
    }

    fun dungPhat() {
        viecPhat?.cancel()
        viecPhat = null
        mp?.release()
        mp = null
        trinhPhat = null
    }

    fun urlBanGhi(tep: String) = baoHieu.urlBanGhi(tep)

    // ================================================================== theo dõi tài xế (khách)
    /** maXe rỗng = thôi theo dõi (quay lại danh sách tài xế). */
    fun theoDoiXe(maXe: String) {
        if (maXe.isEmpty()) {
            theoDoi.dung()
            theoDoi.maXe = ""
            return
        }
        theoDoi.batDau(caiDat.host, caiDat.congMqtt.toIntOrNull() ?: 1883, caiDat.ma, maXe)
    }

    override fun onCleared() {
        tatChuong()
        ketThucAmThanh()
        dungPhat()
        taiXe.dong()
        theoDoi.dung()
        baoHieu.dong()
    }
}
