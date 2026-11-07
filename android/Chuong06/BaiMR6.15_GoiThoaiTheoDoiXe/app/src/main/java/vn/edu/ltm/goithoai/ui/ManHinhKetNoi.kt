package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import vn.edu.ltm.goithoai.CaiDat

/** Màn hình "Kết nối máy chủ" (thiết kế Main). */
@Composable
fun ManHinhKetNoi(c: CaiDat, dangKetNoi: Boolean, loi: String?, doi: (CaiDat) -> Unit, ketNoi: () -> Unit) {
    Column(
        Modifier.fillMaxSize().background(Mau.nen).systemBarsPadding().imePadding()
            .verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 20.dp),
    ) {
        Text("Gọi thoại LTM", style = kieu(26, FontWeight.Bold))
        Spacer(Modifier.height(6.dp))
        Text("Kết nối tới máy chủ trung gian trong phòng lab để gọi cho người trong danh bạ.",
            style = kieu(14, mau = Mau.chuPhu))
        Spacer(Modifier.height(24.dp))

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            O("Địa chỉ IP máy chủ", c.host, Modifier.weight(2.2f), KeyboardType.Uri) { doi(c.copy(host = it)) }
            O("Cổng", c.cong, Modifier.weight(1f), KeyboardType.Number) { doi(c.copy(cong = it.filter(Char::isDigit))) }
        }
        O("Tên hiển thị", c.ten) { doi(c.copy(ten = it)) }
        O("Mã người dùng (số điện thoại trong lab)", c.ma, kieuPhim = KeyboardType.Phone) { doi(c.copy(ma = it)) }

        Spacer(Modifier.height(14.dp))
        Text("Vai trò", style = kieu(13, FontWeight.Medium, Mau.chuPhu))
        Spacer(Modifier.height(6.dp))
        ChonMot(listOf("Khách", "Tài xế"), if (c.vaiTro == "tai_xe") 1 else 0, Modifier.fillMaxWidth()) {
            doi(c.copy(vaiTro = if (it == 1) "tai_xe" else "khach"))
        }
        if (c.vaiTro == "tai_xe") {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                O("Biển số", c.bienSo, Modifier.weight(1f)) { doi(c.copy(bienSo = it)) }
                O("Xe", c.xe, Modifier.weight(1.4f)) { doi(c.copy(xe = it)) }
            }
            O("Cổng MQTT (vị trí)", c.congMqtt, kieuPhim = KeyboardType.Number) { doi(c.copy(congMqtt = it.filter(Char::isDigit))) }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                O("Mã chuyến", c.chuyen, Modifier.weight(1f)) { doi(c.copy(chuyen = it)) }
                O("Điểm đón", c.diemDon, Modifier.weight(1.4f)) { doi(c.copy(diemDon = it)) }
            }
        }

        Spacer(Modifier.height(12.dp))
        Row(
            Modifier.fillMaxWidth().clip12().clickable { doi(c.copy(dongYGhiAm = !c.dongYGhiAm)) },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(c.dongYGhiAm, { doi(c.copy(dongYGhiAm = it)) },
                colors = CheckboxDefaults.colors(checkedColor = Mau.xanh))
            Text("Tôi đồng ý cuộc gọi được máy chủ ghi âm (dùng cho bài thực hành).", style = kieu(14))
        }
        Row(Modifier.fillMaxWidth().padding(start = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Lock, null, Modifier.size(18.dp), tint = Mau.chuPhu)
            Spacer(Modifier.width(8.dp))
            Text("Mã hóa TLS (wss://)", style = kieu(14), modifier = Modifier.weight(1f))
            Switch(c.tls, { doi(c.copy(tls = it)) })
        }
        if (c.tls) {
            O("Vân tay SHA-256 chứng chỉ (để trống: tin lần đầu)", c.vanTay) { doi(c.copy(vanTay = it)) }
        }

        if (loi != null) {
            Spacer(Modifier.height(12.dp))
            Text(loi, style = kieu(14, FontWeight.Medium, Mau.do_))
        }
        Spacer(Modifier.height(20.dp))
        Button(
            ketNoi, Modifier.fillMaxWidth().height(56.dp), enabled = !dangKetNoi,
            shape = RoundedCornerShape(28.dp), colors = ButtonDefaults.buttonColors(containerColor = Mau.xanh),
        ) {
            if (dangKetNoi) {
                CircularProgressIndicator(Modifier.size(22.dp), color = Mau.trang, strokeWidth = 2.dp)
                Spacer(Modifier.width(12.dp))
                Text("Đang kết nối…", style = kieu(16, FontWeight.SemiBold, Mau.trang))
            } else {
                Text("Kết nối", style = kieu(16, FontWeight.SemiBold, Mau.trang))
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(
            "${if (c.tls) "wss" else "ws"}://${c.host}:${c.cong}/ws" +
                if (c.tls) " · chứng chỉ tự ký của phòng lab" else " · không mã hóa (chỉ dùng trong lab)",
            style = kieu(12, mau = Mau.chuPhu), modifier = Modifier.align(Alignment.CenterHorizontally),
        )
    }
}

private fun Modifier.clip12() = this.then(Modifier.clip(RoundedCornerShape(12.dp)))

/** Ô nhập một dòng có nhãn phía trên. */
@Composable
private fun O(
    nhan: String, giaTri: String, modifier: Modifier = Modifier.fillMaxWidth(),
    kieuPhim: KeyboardType = KeyboardType.Text, doi: (String) -> Unit,
) {
    Column(modifier.padding(top = 12.dp)) {
        Text(nhan, style = kieu(13, FontWeight.Medium, Mau.chuPhu))
        Spacer(Modifier.height(6.dp))
        OutlinedTextField(
            giaTri, doi, Modifier.fillMaxWidth(), singleLine = true, textStyle = kieu(16),
            shape = RoundedCornerShape(12.dp), keyboardOptions = KeyboardOptions(keyboardType = kieuPhim),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Mau.xanh, unfocusedBorderColor = Mau.vienO,
                focusedContainerColor = Mau.trang, unfocusedContainerColor = Mau.trang,
            ),
        )
    }
}
