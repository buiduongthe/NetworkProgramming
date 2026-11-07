package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Call
import androidx.compose.material.icons.filled.DirectionsCar
import androidx.compose.material.icons.filled.Science
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import vn.edu.ltm.goithoai.HinhHoc
import vn.edu.ltm.goithoai.KhachTheoDoi
import vn.edu.ltm.goithoai.LienHe
import vn.edu.ltm.goithoai.TaiXeViTri
import vn.edu.ltm.goithoai.soVn

/** Đồng hồ cập nhật mỗi giây (cho "Cập nhật N giây trước"). */
@Composable
private fun bayGioMoiGiay(): Long {
    var t by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            t = System.currentTimeMillis()
            delay(1000)
        }
    }
    return t
}

// ====================================================================== TÀI XẾ (thiết kế TaiXeGpsThat / TaiXeGiaLap)
@Composable
fun ManHinhTaiXe(
    tx: TaiXeViTri, mayChu: String, dangXuat: () -> Unit, xinQuyenGps: () -> Unit,
    chonGpx: () -> Unit, goiKhach: () -> Unit,
) {
    val giaLap = tx.cheDo == "gia_lap"
    val bayGio = bayGioMoiGiay()
    Column(Modifier.fillMaxSize()) {
        DauTrang(if (giaLap) "Giả lập vị trí" else "Vị trí của tôi", mayChu, dangXuat)
        Box(Modifier.fillMaxWidth().height(240.dp)) {
            BanDo(
                Modifier.fillMaxSize(), tuyen = if (giaLap) tx.tuyen?.diem ?: emptyList() else emptyList(),
                vet = tx.vet, xe = tx.viTri, huongXe = tx.huongDo,
                dich = if (giaLap && tx.tuyen?.vong == false) tx.tuyen?.diem?.last() else null,
                nhanDich = tx.tuyen?.diemDen ?: "", mauXe = if (giaLap) Mau.cam else Mau.xanh,
            )
            Box(Modifier.align(Alignment.TopStart).padding(12.dp)) {
                when {
                    tx.anViTri -> Nhan("Đang ẩn vị trí", Mau.trang, Mau.chuPhu)
                    giaLap -> Nhan("Vị trí giả lập – chỉ dùng trong lab", Mau.camNen, Mau.camChu, Icons.Filled.Science)
                    else -> Nhan("Đang nhận chuyến", Mau.xanh, Mau.trang, Icons.Filled.DirectionsCar)
                }
            }
        }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Nguồn vị trí", style = kieu(13, FontWeight.Medium, Mau.chuPhu))
            ChonMot(listOf("GPS thật", "Giả lập"), if (giaLap) 1 else 0, Modifier.fillMaxWidth()) {
                if (it == 0) {
                    tx.chonCheDo("gps")
                    xinQuyenGps()                    // MainActivity xin quyền rồi gọi tx.batGps()
                } else tx.chonCheDo("gia_lap")
            }
            if (giaLap) PhanGiaLap(tx, chonGpx) else PhanGpsThat(tx, bayGio)
            Text(tx.mqtt, style = kieu(12, mau = if (tx.mqttOk) Mau.chuPhu else Mau.do_))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedButton(tx::doiAnViTri, Modifier.weight(1f).height(48.dp), shape = RoundedCornerShape(24.dp)) {
                    Text(if (tx.anViTri) "Hiện vị trí" else "Tạm ẩn vị trí", style = kieu(14, FontWeight.SemiBold, Mau.chu))
                }
                Button(goiKhach, Modifier.weight(1f).height(48.dp), shape = RoundedCornerShape(24.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Mau.xanh)) {
                    Icon(Icons.Filled.Call, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Gọi khách", style = kieu(14, FontWeight.SemiBold, Mau.trang))
                }
            }
        }
    }
}

@Composable
private fun LuoiThongKe(vararg o: Pair<String, String>) {
    o.toList().chunked(2).forEach { hang ->
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            hang.forEach { (k, v) -> OThongKe(k, v, Modifier.weight(1f)) }
            if (hang.size == 1) Spacer(Modifier.weight(1f))
        }
    }
}

@Composable
private fun PhanGpsThat(tx: TaiXeViTri, bayGio: Long) {
    val p = tx.viTri
    LuoiThongKe(
        "Tốc độ" to "${tx.tocDoHienTai.toInt()} km/h",
        "Độ chính xác" to (if (p == null) "–" else "± ${tx.doChinhXac.toInt()} m"),
        "Điểm đã gửi" to "${tx.soDiem}",
        "Gửi lần cuối" to (if (tx.guiLanCuoi == 0L) "–" else "${(bayGio - tx.guiLanCuoi).coerceAtLeast(0) / 1000} giây trước"),
    )
    GhiChuSang(
        (if (tx.gpsTrangThai.isNotEmpty()) tx.gpsTrangThai + ". " else "") +
            "Mỗi ${tx.chuKy} giây app gửi một điểm lên chủ đề xe/…/vitri (MQTT QoS 0, retain); khách đã đặt xe này nhận ngay.",
    )
}

@Composable
private fun PhanGiaLap(tx: TaiXeViTri, chonGpx: () -> Unit) {
    val t = tx.tuyen
    Text("Tuyến mẫu", style = kieu(13, FontWeight.Medium, Mau.chuPhu))
    tx.tuyenMau.forEachIndexed { i, tu ->
        DongChon("${tu.ten} (${soVn(tu.dai / 1000)} km${if (tu.vong) ", vòng lặp" else ""})", tx.tuyenChon == i) {
            tx.dungGiaLap(); tx.tuyenChon = i
        }
    }
    DongChon(tx.tuyenGpx?.let { "Tệp GPX: ${it.ten} (${soVn(it.dai / 1000)} km)" } ?: "Tệp GPX tự nạp…", tx.tuyenChon == -1) {
        if (tx.tuyenGpx == null) chonGpx() else { tx.dungGiaLap(); tx.tuyenChon = -1 }
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text("Tốc độ", style = kieu(13, FontWeight.Medium, Mau.chuPhu), modifier = Modifier.weight(1f))
        Text("${tx.tocDo.toInt()} km/h", style = kieu(14, FontWeight.SemiBold))
    }
    Slider(
        tx.tocDo.toFloat(), { tx.tocDo = it.toDouble() }, valueRange = 5f..80f, steps = 14,
        colors = SliderDefaults.colors(thumbColor = Mau.cam, activeTrackColor = Mau.cam),
    )
    Text("Chu kỳ gửi", style = kieu(13, FontWeight.Medium, Mau.chuPhu))
    val chuKy = listOf(1, 2, 5)
    ChonMot(chuKy.map { "$it giây" }, chuKy.indexOf(tx.chuKy).coerceAtLeast(0), Modifier.fillMaxWidth()) { tx.chuKy = chuKy[it] }
    Text("Nhiễu GPS", style = kieu(13, FontWeight.Medium, Mau.chuPhu))
    val nhieu = listOf(5, 0, 20)
    ChonMot(listOf("± 5 m", "Không", "± 20 m"), nhieu.indexOf(tx.nhieu).coerceAtLeast(0), Modifier.fillMaxWidth()) {
        tx.nhieu = nhieu[it]
    }
    if (t != null) {
        val conLai = (t.dai - tx.daChayM).coerceAtLeast(0.0)
        GhiChuSang(
            if (t.vong) "Đã chạy ${soVn(tx.daChayM / 1000)} km (tuyến vòng ${soVn(t.dai / 1000)} km) · ${tx.soDiem} điểm đã gửi"
            else if (tx.daChayM > 0 && conLai <= 0.0) "Đã tới ${t.diemDen} · ${tx.soDiem} điểm đã gửi"
            else "Đã chạy ${soVn(tx.daChayM / 1000)} / ${soVn(t.dai / 1000)} km · còn khoảng " +
                "${HinhHoc.uocPhut(conLai, tx.tocDo)} phút · ${tx.soDiem} điểm đã gửi",
        )
    }
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        val xong = t != null && !t.vong && tx.daChayM >= t.dai
        Button(
            {
                when {
                    xong -> { tx.dungGiaLap(); tx.batGiaLap() }
                    !tx.dangGiaLap -> tx.batGiaLap()
                    else -> tx.doiTamDung()
                }
            },
            Modifier.weight(1f).height(48.dp), enabled = t != null, shape = RoundedCornerShape(24.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Mau.cam),
        ) {
            Text(
                when {
                    xong -> "Chạy lại"
                    !tx.dangGiaLap -> "Bắt đầu giả lập"
                    tx.tamDung -> "Tiếp tục"
                    else -> "Tạm dừng"
                }, style = kieu(14, FontWeight.SemiBold, Mau.trang),
            )
        }
        OutlinedButton(tx::dungGiaLap, Modifier.weight(1f).height(48.dp), enabled = tx.dangGiaLap || tx.daChayM > 0,
            shape = RoundedCornerShape(24.dp)) {
            Text("Dừng giả lập", style = kieu(14, FontWeight.SemiBold, Mau.chu))
        }
    }
}

@Composable
private fun DongChon(chu: String, chon: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
            .background(if (chon) Mau.camNen else Mau.trang)
            .border(1.dp, if (chon) Mau.cam else Mau.vien, RoundedCornerShape(12.dp))
            .clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(18.dp).clip(RoundedCornerShape(9.dp))
                .border(2.dp, if (chon) Mau.cam else Mau.vienO, RoundedCornerShape(9.dp)).padding(4.dp)
                .clip(RoundedCornerShape(5.dp)).background(if (chon) Mau.cam else Color.Transparent),
        )
        Spacer(Modifier.width(10.dp))
        Text(chu, style = kieu(14, if (chon) FontWeight.SemiBold else FontWeight.Normal))
    }
}

@Composable
private fun GhiChuSang(chu: String) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Mau.xanhNhat).padding(12.dp),
    ) {
        Icon(Icons.Outlined.Info, null, Modifier.size(18.dp), tint = Mau.xanhDam)
        Spacer(Modifier.width(10.dp))
        Text(chu, style = kieu(13, mau = Mau.xanhDam))
    }
}

// ====================================================================== KHÁCH (thiết kế KhachTheoDoi)
@Composable
fun ManHinhKhach(
    kh: KhachTheoDoi, danhBa: List<LienHe>, mayChu: String, diemDon: String,
    dangXuat: () -> Unit, theoDoi: (String) -> Unit, goiTaiXe: () -> Unit, chiaSe: () -> Unit,
) {
    if (kh.maXe.isEmpty()) {
        ChonTaiXe(danhBa, mayChu, dangXuat, theoDoi)
        return
    }
    val bayGio = bayGioMoiGiay()
    val v = kh.viTri
    val taiXe = danhBa.firstOrNull { it.laTaiXe && (it.maXe == kh.maXe || it.ma == kh.maXe) }
    val conLaiM = v?.let { x ->
        x.conLaiM ?: x.dich?.let { HinhHoc.khoangCach(doubleArrayOf(x.lat, x.lon), it) * 1.3 }   // ×1,3: đường thực ≈ đường chim bay × 1,3
    }
    Column(Modifier.fillMaxSize()) {
        DauTrang("Chuyến đi", mayChu, dangXuat)
        Box(Modifier.fillMaxWidth().weight(1f)) {
            BanDo(
                Modifier.fillMaxSize(), vet = kh.vet, xe = v?.let { doubleArrayOf(it.lat, it.lon) },
                huongXe = v?.huong ?: 0.0, dich = v?.dich, nhanDich = diemDon.ifEmpty { v?.dichTen ?: "" },
                mauXe = if (v?.nguon == "gia_lap") Mau.cam else Mau.xanh,
            )
            Box(Modifier.align(Alignment.TopStart).padding(12.dp)) {
                val s = when {
                    kh.anViTri -> "Tài xế đang tạm ẩn vị trí"
                    kh.trangThai == "mat_ket_noi" -> "Tài xế mất kết nối (Last Will)"
                    v == null -> "Đang chờ vị trí…"
                    else -> "Cập nhật ${((bayGio - kh.nhanLuc) / 1000).coerceAtLeast(0)} giây trước" +
                        if (v.nguon == "gia_lap") " · giả lập" else ""
                }
                Nhan(s, Mau.trang, if (kh.trangThai == "mat_ket_noi") Mau.do_ else Mau.chu)
            }
        }
        Column(Modifier.background(Mau.trang).padding(20.dp)) {
            Row(verticalAlignment = Alignment.Bottom) {
                Text(if (conLaiM != null && conLaiM < 30) "Tài xế đã tới" else "Tài xế đang tới",
                    style = kieu(20, FontWeight.Bold), modifier = Modifier.weight(1f))
                if (v != null && conLaiM != null && conLaiM >= 30)
                    Text("${HinhHoc.uocPhut(conLaiM, v.tocDo)} phút", style = kieu(20, FontWeight.Bold, Mau.xanh))
            }
            Spacer(Modifier.height(4.dp))
            Text(
                (if (conLaiM != null) "Còn ${soVn(conLaiM / 1000)} km" else "Chưa biết khoảng cách") +
                    (diemDon.ifEmpty { v?.dichTen ?: "" }.let { if (it.isNotEmpty()) " · đón tại $it" else "" }),
                style = kieu(14, mau = Mau.chuPhu),
            )
            Spacer(Modifier.height(14.dp))
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).border(1.dp, Mau.vien, RoundedCornerShape(14.dp))
                    .padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val ten = taiXe?.ten ?: v?.ten?.ifEmpty { null } ?: kh.maXe
                AnhDaiDien(ten, 44.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(ten, style = kieu(16, FontWeight.SemiBold))
                    Text(v?.xe?.ifEmpty { null } ?: taiXe?.moTa ?: "", style = kieu(13, mau = Mau.chuPhu),
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Nhan(kh.maXe, Mau.nen, Mau.chu)
            }
            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(goiTaiXe, Modifier.weight(1f).height(48.dp), shape = RoundedCornerShape(24.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Mau.xanh)) {
                    Icon(Icons.Filled.Call, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Gọi tài xế", style = kieu(14, FontWeight.SemiBold, Mau.trang))
                }
                OutlinedButton(chiaSe, Modifier.weight(1f).height(48.dp), shape = RoundedCornerShape(24.dp)) {
                    Icon(Icons.Filled.Share, null, Modifier.size(18.dp), tint = Mau.chu)
                    Spacer(Modifier.width(6.dp))
                    Text("Chia sẻ chuyến", style = kieu(14, FontWeight.SemiBold, Mau.chu))
                }
            }
            Spacer(Modifier.height(6.dp))
            Text(
                kh.mqtt + " · " + "đổi tài xế",
                style = kieu(12, mau = Mau.chuPhu),
                modifier = Modifier.clickable { theoDoi("") },
            )
        }
    }
}

/** Khách chưa chọn xe: liệt kê tài xế trong danh bạ (trên ứng dụng thật, xe được gán khi đặt chuyến). */
@Composable
private fun ChonTaiXe(danhBa: List<LienHe>, mayChu: String, dangXuat: () -> Unit, theoDoi: (String) -> Unit) {
    val ds = danhBa.filter { it.laTaiXe }
    Column(Modifier.fillMaxSize()) {
        DauTrang("Chuyến đi", mayChu, dangXuat)
        Text(
            "Chọn tài xế của chuyến để theo dõi vị trí. App đăng ký (SUBSCRIBE) hai chủ đề MQTT " +
                "xe/<mã xe>/vitri và xe/<mã xe>/trangthai.",
            style = kieu(14, mau = Mau.chuPhu), modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
        )
        if (ds.isEmpty()) Text("Chưa có tài xế nào trong danh bạ.", style = kieu(14, mau = Mau.chuPhu),
            modifier = Modifier.padding(20.dp))
        ds.forEach { lh ->
            Row(
                Modifier.fillMaxWidth().clickable { theoDoi(lh.maXe.ifEmpty { lh.ma }) }
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AnhDaiDien(lh.ten, 44.dp, chamTrucTuyen = lh.trucTuyen)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(lh.ten, style = kieu(16, FontWeight.SemiBold))
                    Text(lh.moTa, style = kieu(13, mau = Mau.chuPhu), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Nhan("xe/${lh.maXe}", Mau.xanhNhat, Mau.xanhDam)
            }
        }
    }
}
