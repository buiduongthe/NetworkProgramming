package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Call
import androidx.compose.material.icons.filled.CallEnd
import androidx.compose.material.icons.filled.FiberManualRecord
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MicOff
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.SignalCellularAlt
import androidx.compose.material3.Icon
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import vn.edu.ltm.goithoai.CuocGoi
import vn.edu.ltm.goithoai.TrangThaiGoi
import vn.edu.ltm.goithoai.mmss
import vn.edu.ltm.goithoai.soVn

/** Chọn màn hình theo trạng thái cuộc gọi. */
@Composable
fun ManHinhCuocGoi(
    c: CuocGoi, cupMay: () -> Unit, traLoi: () -> Unit, tuChoi: (String) -> Unit,
    tatMic: () -> Unit, loaNgoai: () -> Unit, giuMay: () -> Unit,
) {
    Column(
        Modifier.fillMaxSize().background(Mau.toi).systemBarsPadding().padding(horizontal = 24.dp, vertical = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        when (c.trangThai) {
            TrangThaiGoi.DANG_GOI, TrangThaiGoi.DO_CHUONG -> GoiDi(c, cupMay, loaNgoai, tatMic)
            TrangThaiGoi.CUOC_DEN -> CuocGoiDen(c, traLoi, tuChoi)
            TrangThaiGoi.DANG_KET_NOI, TrangThaiGoi.DANG_NOI -> TrongCuocGoi(c, cupMay, tatMic, loaNgoai, giuMay)
        }
    }
}

@Composable
private fun NguoiKia(c: CuocGoi, dong3: String, co: Int = 112) {
    AnhDaiDien(c.doiPhuong.ten, co.dp, tren = Mau.theToi)
    Spacer(Modifier.height(18.dp))
    Text(c.doiPhuong.ten, style = kieu(28, FontWeight.Bold, Mau.trang), textAlign = TextAlign.Center)
    Spacer(Modifier.height(4.dp))
    Text(c.doiPhuong.moTa, style = kieu(14, mau = Mau.chuSangPhu), textAlign = TextAlign.Center)
    Spacer(Modifier.height(10.dp))
    Text(dong3, style = kieu(16, FontWeight.Medium, Mau.chuNhan), textAlign = TextAlign.Center)
}

@Composable
private fun GhiChu(chu: String) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Mau.theToi).padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.Info, null, Modifier.size(18.dp), tint = Mau.chuSangPhu)
        Spacer(Modifier.width(10.dp))
        Text(chu, style = kieu(13, mau = Mau.chuSangPhu))
    }
}

// ---------------------------------------------------------------------- gọi đi (thiết kế GoiDi)
@Composable
private fun ColumnScope.GoiDi(c: CuocGoi, cupMay: () -> Unit, loaNgoai: () -> Unit, tatMic: () -> Unit) {
    Text("Cuộc gọi thoại qua ứng dụng", style = kieu(13, FontWeight.Medium, Mau.chuSangPhu))
    Spacer(Modifier.height(48.dp))
    NguoiKia(c, if (c.trangThai == TrangThaiGoi.DANG_GOI) "Đang kết nối…" else "Đang đổ chuông…")
    Spacer(Modifier.weight(1f))
    GhiChu("Số điện thoại của hai bên được ẩn: cuộc gọi đi qua máy chủ trung gian, không lộ địa chỉ IP của nhau.")
    Spacer(Modifier.height(28.dp))
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.Top) {
        NutTron(Icons.AutoMirrored.Filled.VolumeUp, "Loa ngoài", if (c.loaNgoai) Mau.trang else Mau.theToi,
            if (c.loaNgoai) Mau.toi else Mau.trang, onClick = loaNgoai)
        NutTron(Icons.Filled.CallEnd, "Hủy", Mau.do_, Mau.trang, co = 76.dp, onClick = cupMay)
        NutTron(if (c.tatMic) Icons.Filled.MicOff else Icons.Filled.Mic, "Micro", if (c.tatMic) Mau.trang else Mau.theToi,
            if (c.tatMic) Mau.toi else Mau.trang, onClick = tatMic)
    }
    Spacer(Modifier.height(16.dp))
}

// ---------------------------------------------------------------------- cuộc gọi đến (thiết kế CuocGoiDen)
@Composable
private fun ColumnScope.CuocGoiDen(c: CuocGoi, traLoi: () -> Unit, tuChoi: (String) -> Unit) {
    Text("Cuộc gọi đến", style = kieu(13, FontWeight.Medium, Mau.chuSangPhu))
    Spacer(Modifier.height(48.dp))
    NguoiKia(c, "Gọi qua ứng dụng…")
    Spacer(Modifier.weight(1f))
    if (c.ghiAm) {
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Mau.camNen).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Filled.FiberManualRecord, null, Modifier.size(16.dp), tint = Mau.do_)
            Spacer(Modifier.width(10.dp))
            Text("Cuộc gọi này sẽ được máy chủ ghi âm. Trả lời nghĩa là bạn đồng ý.", style = kieu(13, mau = Mau.camChu))
        }
        Spacer(Modifier.height(16.dp))
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally)) {
        TraLoiNhanh("Tôi đang tới") { tuChoi("Tôi đang tới") }
        TraLoiNhanh("Gọi lại sau") { tuChoi("Gọi lại sau") }
    }
    Spacer(Modifier.height(28.dp))
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
        NutTron(Icons.Filled.CallEnd, "Từ chối", Mau.do_, Mau.trang, co = 76.dp) { tuChoi("Từ chối") }
        NutTron(Icons.Filled.Call, "Trả lời", Mau.xanhTraLoi, Mau.trang, co = 76.dp, onClick = traLoi)
    }
    Spacer(Modifier.height(16.dp))
}

@Composable
private fun TraLoiNhanh(chu: String, onClick: () -> Unit) {
    Box(
        Modifier.clip(RoundedCornerShape(50)).border(1.dp, Mau.theToi, RoundedCornerShape(50))
            .clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 9.dp),
    ) {
        Text(chu, style = kieu(13, FontWeight.Medium, Mau.chuSangPhu))
    }
}

// ---------------------------------------------------------------------- trong cuộc gọi (thiết kế TrongCuocGoi)
@Composable
private fun ColumnScope.TrongCuocGoi(c: CuocGoi, cupMay: () -> Unit, tatMic: () -> Unit, loaNgoai: () -> Unit, giuMay: () -> Unit) {
    var bayGio by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            bayGio = System.currentTimeMillis()
            delay(500)
        }
    }
    val (chatLuong, mauCL) = when {
        c.trangThai == TrangThaiGoi.DANG_KET_NOI -> "Đang nối âm thanh…" to Mau.chuSangPhu
        c.rttMs == null -> "Đang đo mạng…" to Mau.chuSangPhu
        c.rttMs < 150 && (c.matGoi ?: 0.0) < 2 -> "Mạng tốt" to Mau.chuNhan
        c.rttMs < 400 && (c.matGoi ?: 0.0) < 8 -> "Mạng trung bình" to Color(0xFFFFD27A)
        else -> "Mạng yếu" to Color(0xFFFF9C8F)
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        if (c.ghiAm) Nhan("Đang ghi âm", Color(0x33C62828), Color(0xFFFFB4AB), Icons.Filled.FiberManualRecord)
        Spacer(Modifier.weight(1f))
        Icon(Icons.Outlined.SignalCellularAlt, null, Modifier.size(16.dp), tint = mauCL)
        Spacer(Modifier.width(6.dp))
        Text(chatLuong, style = kieu(13, FontWeight.Medium, mauCL))
    }
    Spacer(Modifier.height(36.dp))
    val trangThai = when {
        c.giuMay -> "Bạn đang giữ máy"
        c.doiPhuongGiu -> "${c.doiPhuong.ten} đang giữ máy"
        c.doiPhuongTatMic -> "${c.doiPhuong.ten} đã tắt micro"
        else -> mmss((bayGio - c.batDau) / 1000.0)
    }
    NguoiKia(c, trangThai, 96)
    if (c.giuMay || c.doiPhuongGiu || c.doiPhuongTatMic) {
        Text(mmss((bayGio - c.batDau) / 1000.0), style = kieu(14, mau = Mau.chuSangPhu))
    }
    Spacer(Modifier.weight(1f))
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OThongKe("Khứ hồi (RTT)", c.rttMs?.let { "${it.toInt()} ms" } ?: "–", Modifier.weight(1f), toi = true)
            OThongKe("Mất gói", c.matGoi?.let { "${soVn(it)} %" } ?: "–", Modifier.weight(1f), toi = true)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OThongKe("Mã hóa", "Opus" + (c.kbps?.let { " ${it.toInt()} kbit/s" } ?: ""), Modifier.weight(1f), toi = true)
            OThongKe("Đường đi", "Qua máy chủ (SFU)", Modifier.weight(1f), toi = true)
        }
    }
    Spacer(Modifier.height(28.dp))
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.Top) {
        NutTron(if (c.tatMic) Icons.Filled.MicOff else Icons.Filled.Mic, if (c.tatMic) "Bật micro" else "Tắt micro",
            if (c.tatMic) Mau.trang else Mau.theToi, if (c.tatMic) Mau.toi else Mau.trang, onClick = tatMic)
        NutTron(Icons.AutoMirrored.Filled.VolumeUp, "Loa ngoài", if (c.loaNgoai) Mau.trang else Mau.theToi,
            if (c.loaNgoai) Mau.toi else Mau.trang, onClick = loaNgoai)
        NutTron(if (c.giuMay) Icons.Filled.PlayArrow else Icons.Filled.Pause, if (c.giuMay) "Tiếp tục" else "Giữ máy",
            if (c.giuMay) Mau.trang else Mau.theToi, if (c.giuMay) Mau.toi else Mau.trang, onClick = giuMay)
    }
    Spacer(Modifier.height(24.dp))
    NutTron(Icons.Filled.CallEnd, null, Mau.do_, Mau.trang, co = 72.dp, moTa = "Kết thúc", onClick = cupMay)
    Spacer(Modifier.height(16.dp))
}
