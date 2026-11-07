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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Call
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import vn.edu.ltm.goithoai.LienHe

/** Đầu trang dùng chung cho các thẻ: tiêu đề + nhãn máy chủ + nút đăng xuất. */
@Composable
fun DauTrang(tieuDe: String, mayChu: String, dangXuat: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp, top = 12.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(tieuDe, style = kieu(24, FontWeight.Bold), modifier = Modifier.weight(1f))
        Nhan(mayChu, Mau.xanhNhat, Mau.xanhDam)
        IconButton(dangXuat) { Icon(Icons.AutoMirrored.Filled.Logout, "Ngắt kết nối", tint = Mau.chuPhu) }
    }
}

/** Màn hình Danh bạ (thiết kế DanhBa). */
@Composable
fun ManHinhDanhBa(ds: List<LienHe>, mayChu: String, dangXuat: () -> Unit, goi: (LienHe) -> Unit) {
    var tim by remember { mutableStateOf("") }
    var loc by remember { mutableIntStateOf(0) }
    val soTrucTuyen = ds.count { it.trucTuyen }
    val hien = ds.filter { (loc == 0 || it.trucTuyen) && (tim.isBlank() || it.ten.contains(tim, true) || it.ma.contains(tim)) }
    Column(Modifier.fillMaxSize()) {
        DauTrang("Danh bạ", mayChu, dangXuat)
        OutlinedTextField(
            tim, { tim = it }, Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp),
            placeholder = { Text("Tìm trong danh bạ", style = kieu(15, mau = Mau.chuPhu)) },
            leadingIcon = { Icon(Icons.Outlined.Search, null, tint = Mau.chuPhu) },
            singleLine = true, shape = RoundedCornerShape(24.dp), textStyle = kieu(15),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Mau.xanh, unfocusedBorderColor = Mau.vien,
                focusedContainerColor = Mau.trang, unfocusedContainerColor = Mau.trang,
            ),
        )
        Row(Modifier.padding(horizontal = 20.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ChipLoc("Tất cả (${ds.size})", loc == 0) { loc = 0 }
            ChipLoc("Đang trực tuyến ($soTrucTuyen)", loc == 1) { loc = 1 }
        }
        if (hien.isEmpty()) {
            Text(
                if (ds.isEmpty()) "Chưa có ai khác đăng nhập vào máy chủ." else "Không tìm thấy.",
                style = kieu(14, mau = Mau.chuPhu), modifier = Modifier.padding(24.dp),
            )
        }
        LazyColumn(Modifier.weight(1f)) {
            items(hien, key = { it.ma }) { lh -> DongLienHe(lh) { goi(lh) } }
        }
    }
}

@Composable
fun ChipLoc(chu: String, chon: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.clip(RoundedCornerShape(50))
            .background(if (chon) Mau.xanh else Mau.trang)
            .border(1.dp, if (chon) Mau.xanh else Mau.vien, RoundedCornerShape(50))
            .clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 7.dp),
    ) {
        Text(chu, style = kieu(13, FontWeight.Medium, if (chon) Mau.trang else Mau.chu))
    }
}

@Composable
private fun DongLienHe(lh: LienHe, goi: () -> Unit) {
    val trangThai = when {
        !lh.trucTuyen -> "Ngoại tuyến"
        lh.ban -> "Đang trong cuộc gọi"
        else -> "Trực tuyến"
    } + lh.moTa.substringAfter(" · ", "").let { if (it.isNotEmpty()) " · $it" else "" }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnhDaiDien(lh.ten, 48.dp, chamTrucTuyen = lh.trucTuyen)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(lh.ten, style = kieu(16, FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false))
                Spacer(Modifier.width(8.dp))
                if (lh.laTaiXe) Nhan("Tài xế", Mau.xanhNhat, Mau.xanhDam) else Nhan("Khách", Mau.khachNen, Mau.khachChu)
            }
            Spacer(Modifier.height(2.dp))
            Text(trangThai, style = kieu(13, mau = Mau.chuPhu), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Spacer(Modifier.width(10.dp))
        val coTheGoi = lh.trucTuyen && !lh.ban
        Box(
            Modifier.size(44.dp).clip(CircleShape).background(if (coTheGoi) Mau.xanh else Mau.vien)
                .clickable(onClick = goi),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Filled.Call, "Gọi ${lh.ten}", Modifier.size(20.dp), tint = if (coTheGoi) Mau.trang else Mau.chuPhu)
        }
    }
}
