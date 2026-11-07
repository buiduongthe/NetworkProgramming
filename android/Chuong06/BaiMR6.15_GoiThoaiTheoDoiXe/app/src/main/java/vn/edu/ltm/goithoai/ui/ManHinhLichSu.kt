package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.CallMade
import androidx.compose.material.icons.automirrored.filled.CallMissed
import androidx.compose.material.icons.automirrored.filled.CallReceived
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.GraphicEq
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import vn.edu.ltm.goithoai.MucLichSu
import vn.edu.ltm.goithoai.TrinhPhat
import vn.edu.ltm.goithoai.kichThuocDep
import vn.edu.ltm.goithoai.mmss
import vn.edu.ltm.goithoai.moTaKetQua

/** Màn hình Lịch sử và bản ghi âm (thiết kế LichSu). Bản ghi nằm trên máy chủ, app tải về bộ nhớ đệm rồi phát. */
@Composable
fun ManHinhLichSu(
    ds: List<MucLichSu>, maToi: String, mayChu: String, tp: TrinhPhat?,
    dangXuat: () -> Unit, taiLai: () -> Unit, phat: (MucLichSu, String) -> Unit,
    doiPhat: () -> Unit, tua: (Float) -> Unit, moNgoai: (String) -> Unit,
) {
    var loc by remember { mutableIntStateOf(0) }
    val coBanGhi = ds.count { it.banGhi.isNotEmpty() }
    val hien = if (loc == 1) ds.filter { it.banGhi.isNotEmpty() } else ds
    val dangMo = tp?.let { t -> ds.firstOrNull { it.maCuoc == t.maCuoc } }
    Column(Modifier.fillMaxSize()) {
        DauTrang("Lịch sử cuộc gọi", mayChu, dangXuat)
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically,
        ) {
            ChipLoc("Tất cả", loc == 0) { loc = 0 }
            ChipLoc("Có bản ghi ($coBanGhi)", loc == 1) { loc = 1 }
            Spacer(Modifier.weight(1f))
            IconButton(taiLai) { Icon(Icons.Filled.Refresh, "Tải lại", tint = Mau.chuPhu) }
        }
        if (tp != null && dangMo != null) TheTrinhPhat(dangMo, maToi, tp, phat, doiPhat, tua, moNgoai)
        if (hien.isEmpty()) {
            Text("Chưa có cuộc gọi nào.", style = kieu(14, mau = Mau.chuPhu), modifier = Modifier.padding(24.dp))
        }
        LazyColumn(Modifier.weight(1f)) {
            items(hien, key = { it.maCuoc }) { m ->
                DongLichSu(m, maToi, m.maCuoc == tp?.maCuoc) {
                    if (m.banGhi.isNotEmpty()) phat(m, kenhMacDinh(m, maToi))
                }
            }
        }
    }
}

private fun kenhMacDinh(m: MucLichSu, maToi: String) = when {
    "tron" in m.banGhi -> "tron"
    maToi in m.banGhi -> maToi
    else -> m.banGhi.keys.first()
}

@Composable
private fun DongLichSu(m: MucLichSu, maToi: String, dangChon: Boolean, onClick: () -> Unit) {
    val goiDi = m.maGoi == maToi
    val tenKia = if (goiDi) m.tenNghe else m.tenGoi
    val nho = m.ketQua != "hoan_thanh"
    Row(
        Modifier.fillMaxWidth().background(if (dangChon) Mau.xanhNhat else Mau.nen).clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnhDaiDien(tenKia, 44.dp)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(tenKia, style = kieu(16, FontWeight.SemiBold, if (nho && !goiDi) Mau.do_ else Mau.chu))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    when {
                        nho && !goiDi -> Icons.AutoMirrored.Filled.CallMissed
                        goiDi -> Icons.AutoMirrored.Filled.CallMade
                        else -> Icons.AutoMirrored.Filled.CallReceived
                    }, null, Modifier.size(14.dp), tint = if (nho && !goiDi) Mau.do_ else Mau.chuPhu,
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    "${if (goiDi) "Gọi đi" else "Gọi đến"} · ${m.batDau.drop(5).dropLast(3)} · " +
                        if (m.ketQua == "hoan_thanh") mmss(m.thoiLuong) else "${moTaKetQua(m.ketQua)}: ${m.lyDo}",
                    style = kieu(13, mau = Mau.chuPhu), maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (m.banGhi.isNotEmpty()) {
            Spacer(Modifier.width(8.dp))
            Nhan(kichThuocDep(m.kichThuoc), Mau.xanhNhat, Mau.xanhDam, Icons.Filled.GraphicEq)
        }
    }
}

@Composable
private fun TheTrinhPhat(
    m: MucLichSu, maToi: String, tp: TrinhPhat, phat: (MucLichSu, String) -> Unit,
    doiPhat: () -> Unit, tua: (Float) -> Unit, moNgoai: (String) -> Unit,
) {
    val goiDi = m.maGoi == maToi
    val maKia = if (goiDi) m.maNghe else m.maGoi
    val tenKia = if (goiDi) m.tenNghe else m.tenGoi
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp).clip(RoundedCornerShape(16.dp))
            .background(Mau.trang).border(1.dp, Mau.vien, RoundedCornerShape(16.dp)).padding(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("$tenKia · ${m.batDau.drop(5).dropLast(3)}", style = kieu(15, FontWeight.SemiBold), modifier = Modifier.weight(1f))
            Text(mmss(m.thoiLuong), style = kieu(13, mau = Mau.chuPhu))
        }
        Spacer(Modifier.height(12.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(48.dp).clip(CircleShape).background(Mau.xanh).clickable(enabled = !tp.dangTai, onClick = doiPhat),
                contentAlignment = Alignment.Center,
            ) {
                if (tp.dangTai) CircularProgressIndicator(Modifier.size(22.dp), color = Mau.trang, strokeWidth = 2.dp)
                else Icon(if (tp.dangPhat) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                    if (tp.dangPhat) "Tạm dừng" else "Phát", tint = Mau.trang)
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                ThanhTienDo(if (tp.tongMs > 0) tp.viTriMs.toFloat() / tp.tongMs else 0f, tua)
                Spacer(Modifier.height(4.dp))
                Text("${mmss(tp.viTriMs / 1000.0)} / ${mmss(tp.tongMs / 1000.0)}", style = kieu(12, mau = Mau.chuPhu))
            }
        }
        Spacer(Modifier.height(8.dp))
        Text("${tp.tep} · ${kichThuocDep(m.kichThuoc)}", style = kieu(12, mau = Mau.chuPhu), maxLines = 1,
            overflow = TextOverflow.Ellipsis)
        if (tp.loi != null) {
            Spacer(Modifier.height(6.dp))
            Text(tp.loi, style = kieu(12, FontWeight.Medium, Mau.do_))
        }
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            if ("tron" in m.banGhi) ChipLoc("Cả hai", tp.kenh == "tron") { phat(m, "tron") }
            if (maToi in m.banGhi) ChipLoc("Chỉ tôi", tp.kenh == maToi) { phat(m, maToi) }
            if (maKia in m.banGhi) ChipLoc("Chỉ $tenKia", tp.kenh == maKia) { phat(m, maKia) }
            Spacer(Modifier.weight(1f))
            IconButton({ moNgoai(tp.tep) }) {
                Icon(Icons.AutoMirrored.Filled.OpenInNew, "Mở bằng ứng dụng khác", tint = Mau.chuPhu)
            }
        }
    }
}

/** Thanh tiến độ dạng cột (chạm để tua). Các cột chỉ là trang trí – không phải dạng sóng thật của bản ghi. */
@Composable
private fun ThanhTienDo(phan: Float, tua: (Float) -> Unit) {
    val cao = remember { listOf(10, 18, 26, 14, 30, 22, 12, 28, 34, 20, 16, 24, 32, 18, 10, 22, 30, 26, 14, 20, 28, 12, 18, 24, 30, 16, 22, 12, 26, 14) }
    Canvas(
        Modifier.fillMaxWidth().height(36.dp).pointerInput(Unit) {
            detectTapGestures { o -> tua((o.x / size.width).coerceIn(0f, 1f)) }
        },
    ) {
        val n = cao.size
        val buoc = size.width / n
        val rong = buoc * 0.55f
        for (i in 0 until n) {
            val h = size.height * cao[i] / 36f
            drawRoundRect(
                if ((i + 0.5f) / n <= phan) Mau.xanh else Mau.vienO,
                topLeft = Offset(i * buoc + (buoc - rong) / 2, (size.height - h) / 2),
                size = Size(rong, h), cornerRadius = CornerRadius(rong / 2),
            )
        }
    }
}
