package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import vn.edu.ltm.goithoai.R

/** Bảng màu của bản thiết kế đã duyệt (nền sáng, xanh lá chủ đạo; màn hình cuộc gọi nền tối). */
object Mau {
    val nen = Color(0xFFF6F8F7)
    val chu = Color(0xFF13201A)
    val chuPhu = Color(0xFF4A5A52)
    val vien = Color(0xFFE1E7E4)
    val vienO = Color(0xFFB8C4BE)
    val xanh = Color(0xFF0B7A43)
    val xanhDam = Color(0xFF075C32)
    val xanhNhat = Color(0xFFDCEFE4)
    val toi = Color(0xFF0F2A1E)
    val theToi = Color(0xFF1E4A35)
    val chuSangPhu = Color(0xFFB9D3C5)
    val chuNhan = Color(0xFF9FE0BC)
    val do_ = Color(0xFFC62828)
    val xanhTraLoi = Color(0xFF1FA35C)
    val cam = Color(0xFFB26A00)
    val camNen = Color(0xFFFFF4D6)
    val camChu = Color(0xFF6B4A00)
    val banDo = Color(0xFFE6ECE7)
    val khachNen = Color(0xFFE3ECF7)
    val khachChu = Color(0xFF1F4E8C)
    val xam = Color(0xFF8A9690)
    val trang = Color.White
}

val BeVietnamPro = FontFamily(
    Font(R.font.be_vietnam_pro_regular, FontWeight.Normal),
    Font(R.font.be_vietnam_pro_medium, FontWeight.Medium),
    Font(R.font.be_vietnam_pro_semibold, FontWeight.SemiBold),
    Font(R.font.be_vietnam_pro_bold, FontWeight.Bold),
)

private fun TextStyle.voiFont() = copy(fontFamily = BeVietnamPro)

@Composable
fun GiaoDienGoiThoai(noiDung: @Composable () -> Unit) {
    val t = Typography()
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = Mau.xanh, onPrimary = Mau.trang, primaryContainer = Mau.xanhNhat, onPrimaryContainer = Mau.xanhDam,
            secondary = Mau.xanhDam, background = Mau.nen, onBackground = Mau.chu, surface = Mau.trang,
            onSurface = Mau.chu, onSurfaceVariant = Mau.chuPhu, outline = Mau.vienO, outlineVariant = Mau.vien,
            error = Mau.do_,
        ),
        typography = Typography(
            displayLarge = t.displayLarge.voiFont(), displayMedium = t.displayMedium.voiFont(),
            displaySmall = t.displaySmall.voiFont(), headlineLarge = t.headlineLarge.voiFont(),
            headlineMedium = t.headlineMedium.voiFont(), headlineSmall = t.headlineSmall.voiFont(),
            titleLarge = t.titleLarge.voiFont(), titleMedium = t.titleMedium.voiFont(), titleSmall = t.titleSmall.voiFont(),
            bodyLarge = t.bodyLarge.voiFont(), bodyMedium = t.bodyMedium.voiFont(), bodySmall = t.bodySmall.voiFont(),
            labelLarge = t.labelLarge.voiFont(), labelMedium = t.labelMedium.voiFont(), labelSmall = t.labelSmall.voiFont(),
        ),
        content = noiDung,
    )
}

/** Kiểu chữ dùng nhanh: cỡ (sp), độ đậm, màu. */
fun kieu(co: Int, dam: FontWeight = FontWeight.Normal, mau: Color = Mau.chu) =
    TextStyle(fontFamily = BeVietnamPro, fontSize = co.sp, fontWeight = dam, color = mau, lineHeight = (co * 1.35).sp)

/** Ảnh đại diện tròn: chữ cái đầu của tên trên nền màu suy từ tên (ổn định giữa các lần mở). */
@Composable
fun AnhDaiDien(ten: String, co: Dp, tren: Color? = null, chamTrucTuyen: Boolean? = null) {
    val bangMau = listOf(0xFFF7D9C4, 0xFFE6DDF5, 0xFFCDE8D8, 0xFFD6E6F5, 0xFFF5E3B3, 0xFFE1E7E4)
    val nen = tren ?: Color(bangMau[(ten.hashCode() and 0x7fffffff) % bangMau.size])
    Box(Modifier.size(co)) {
        Box(
            Modifier.size(co).clip(CircleShape).background(nen),
            contentAlignment = Alignment.Center,
        ) {
            Text(ten.trim().firstOrNull()?.uppercase() ?: "?", style = kieu((co.value * 0.4f).toInt(), FontWeight.SemiBold,
                if (tren != null) Mau.trang else Mau.chu))
        }
        if (chamTrucTuyen != null) {
            Box(
                Modifier.align(Alignment.BottomEnd).size(co * 0.28f).clip(CircleShape).background(Mau.trang)
                    .padding(2.dp).clip(CircleShape).background(if (chamTrucTuyen) Mau.xanh else Mau.xam),
            )
        }
    }
}

/** Nhãn nhỏ bo tròn. */
@Composable
fun Nhan(chu: String, nen: Color, mauChu: Color, bieuTuong: ImageVector? = null, modifier: Modifier = Modifier) {
    Row(
        modifier.clip(RoundedCornerShape(50)).background(nen).padding(horizontal = 10.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (bieuTuong != null) {
            Icon(bieuTuong, null, Modifier.size(14.dp), tint = mauChu)
            Spacer(Modifier.width(4.dp))
        }
        Text(chu, style = kieu(12, FontWeight.Medium, mauChu))
    }
}

/** Nút tròn có biểu tượng và chú thích bên dưới (màn hình cuộc gọi). */
@Composable
fun NutTron(
    bieuTuong: ImageVector, chuThich: String?, nen: Color, mauBieuTuong: Color, co: Dp = 60.dp,
    mauChu: Color = Mau.chuSangPhu, moTa: String = chuThich ?: "", onClick: () -> Unit,
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier.size(co).clip(CircleShape).background(nen).clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            Icon(bieuTuong, moTa, Modifier.size(co * 0.42f), tint = mauBieuTuong)
        }
        if (chuThich != null) {
            Spacer(Modifier.height(8.dp))
            Text(chuThich, style = kieu(12, FontWeight.Medium, mauChu))
        }
    }
}

/** Ô thống kê: nhãn nhỏ phía trên, giá trị đậm phía dưới. */
@Composable
fun OThongKe(nhan: String, giaTri: String, modifier: Modifier = Modifier, toi: Boolean = false) {
    Column(
        modifier.clip(RoundedCornerShape(12.dp))
            .background(if (toi) Mau.theToi else Mau.trang)
            .then(if (toi) Modifier else Modifier.border(1.dp, Mau.vien, RoundedCornerShape(12.dp)))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(nhan, style = kieu(11, FontWeight.Normal, if (toi) Mau.chuSangPhu else Mau.chuPhu))
        Text(giaTri, style = kieu(15, FontWeight.SemiBold, if (toi) Mau.trang else Mau.chu))
    }
}

/** Nhóm nút chọn một trong nhiều (segmented). */
@Composable
fun ChonMot(cacMuc: List<String>, chon: Int, modifier: Modifier = Modifier, khiChon: (Int) -> Unit) {
    Row(
        modifier.clip(RoundedCornerShape(12.dp)).border(1.dp, Mau.vienO, RoundedCornerShape(12.dp)).padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        cacMuc.forEachIndexed { i, s ->
            Box(
                Modifier.weight(1f).clip(RoundedCornerShape(9.dp))
                    .background(if (i == chon) Mau.xanh else Color.Transparent)
                    .clickable { khiChon(i) }.padding(vertical = 9.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(s, style = kieu(14, FontWeight.Medium, if (i == chon) Mau.trang else Mau.chu))
            }
        }
    }
}
