package vn.edu.ltm.goithoai.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.pow

/**
 * Bản đồ tối giản vẽ bằng Canvas, KHÔNG cần ô bản đồ (tile) từ Internet – phòng lab không phải lúc nào cũng ra mạng.
 * Chiếu phẳng (equirectangular) quanh tâm vùng hiển thị: x = Δlon·cos(lat₀), y = −Δlat, đủ chính xác cho vài km.
 * Vẽ: lưới nền, tuyến (nếu có), vết đã đi, điểm đích, xe (mũi tên theo hướng) và thước tỉ lệ.
 */
@Composable
fun BanDo(
    modifier: Modifier,
    tuyen: List<DoubleArray> = emptyList(),
    vet: List<DoubleArray> = emptyList(),
    xe: DoubleArray? = null,
    huongXe: Double = 0.0,
    dich: DoubleArray? = null,
    nhanDich: String = "",
    mauXe: Color = Mau.xanh,
) {
    val do_ = rememberTextMeasurer()
    Canvas(modifier.background(Mau.banDo)) {
        val tatCa = tuyen + vet + listOfNotNull(xe, dich)
        veLuoi()
        if (tatCa.isEmpty()) return@Canvas
        var minLa = tatCa.minOf { it[0] }; var maxLa = tatCa.maxOf { it[0] }
        var minLo = tatCa.minOf { it[1] }; var maxLo = tatCa.maxOf { it[1] }
        val toiThieu = 0.002                                   // ≈ 200 m: tránh phóng quá to khi chỉ có một điểm
        if (maxLa - minLa < toiThieu) { val g = (minLa + maxLa) / 2; minLa = g - toiThieu / 2; maxLa = g + toiThieu / 2 }
        if (maxLo - minLo < toiThieu) { val g = (minLo + maxLo) / 2; minLo = g - toiThieu / 2; maxLo = g + toiThieu / 2 }
        val la0 = (minLa + maxLa) / 2; val lo0 = (minLo + maxLo) / 2
        val k = cos(la0 * PI / 180)
        val le = 36.dp.toPx()
        val tiLe = minOf((size.width - 2 * le) / ((maxLo - minLo) * k), (size.height - 2 * le) / (maxLa - minLa))
        fun ra(p: DoubleArray) = Offset(
            (size.width / 2 + (p[1] - lo0) * k * tiLe).toFloat(),
            (size.height / 2 - (p[0] - la0) * tiLe).toFloat(),
        )

        if (tuyen.size >= 2) duong(tuyen.map(::ra), Mau.trang, 10.dp.toPx())
        if (tuyen.size >= 2) duong(tuyen.map(::ra), Color(0xFF9DB3A6), 4.dp.toPx(), net = true)
        if (vet.size >= 2) duong(vet.map(::ra), mauXe.copy(alpha = 0.75f), 5.dp.toPx())
        dich?.let {
            val o = ra(it)
            drawCircle(Mau.do_, 9.dp.toPx(), o)
            drawCircle(Mau.trang, 4.dp.toPx(), o)
            if (nhanDich.isNotEmpty()) chu(do_, nhanDich, o + Offset(12.dp.toPx(), -8.dp.toPx()))
        }
        xe?.let {
            val o = ra(it)
            drawCircle(mauXe.copy(alpha = 0.18f), 22.dp.toPx(), o)
            drawCircle(Mau.trang, 13.dp.toPx(), o)
            drawCircle(mauXe, 11.dp.toPx(), o)
            rotate(huongXe.toFloat(), o) {
                val s = 6.dp.toPx()
                val p = Path().apply {
                    moveTo(o.x, o.y - s * 1.2f); lineTo(o.x + s, o.y + s); lineTo(o.x, o.y + s * 0.45f)
                    lineTo(o.x - s, o.y + s); close()
                }
                drawPath(p, Mau.trang)
            }
        }
        thuocTiLe(do_, tiLe)
    }
}

private fun DrawScope.veLuoi() {
    val buoc = 40.dp.toPx()
    var x = 0f
    while (x < size.width) { drawLine(Color(0x14000000), Offset(x, 0f), Offset(x, size.height), 1f); x += buoc }
    var y = 0f
    while (y < size.height) { drawLine(Color(0x14000000), Offset(0f, y), Offset(size.width, y), 1f); y += buoc }
}

private fun DrawScope.duong(ds: List<Offset>, mau: Color, rong: Float, net: Boolean = false) {
    val p = Path().apply {
        moveTo(ds[0].x, ds[0].y)
        for (i in 1 until ds.size) lineTo(ds[i].x, ds[i].y)
    }
    drawPath(p, mau, style = Stroke(rong, cap = StrokeCap.Round, join = StrokeJoin.Round,
        pathEffect = if (net) PathEffect.dashPathEffect(floatArrayOf(rong * 2.5f, rong * 2f)) else null))
}

private fun DrawScope.chu(do_: TextMeasurer, s: String, o: Offset) {
    drawText(do_, s, o, kieu(12, androidx.compose.ui.text.font.FontWeight.Medium, Mau.chu))
}

/** Thước tỉ lệ góc dưới trái: chọn độ dài tròn (1, 2, 5 × 10ⁿ m) gần 1/4 chiều rộng. */
private fun DrawScope.thuocTiLe(do_: TextMeasurer, tiLe: Double) {
    val mPerPx = 111320.0 / tiLe                              // 1 độ vĩ ≈ 111,32 km
    val muonM = mPerPx * size.width / 4
    val mu = 10.0.pow(floor(log10(muonM)))
    val m = listOf(1.0, 2.0, 5.0, 10.0).map { it * mu }.last { it <= max(muonM, mu) }
    val px = (m / mPerPx).toFloat()
    val goc = Offset(16.dp.toPx(), size.height - 16.dp.toPx())
    drawLine(Mau.chu, goc, goc + Offset(px, 0f), 2.dp.toPx())
    drawLine(Mau.chu, goc, goc + Offset(0f, -6.dp.toPx()), 2.dp.toPx())
    drawLine(Mau.chu, goc + Offset(px, 0f), goc + Offset(px, -6.dp.toPx()), 2.dp.toPx())
    chu(do_, if (m >= 1000) "${(m / 1000).toInt()} km" else "${m.toInt()} m", goc + Offset(4.dp.toPx(), -24.dp.toPx()))
}
