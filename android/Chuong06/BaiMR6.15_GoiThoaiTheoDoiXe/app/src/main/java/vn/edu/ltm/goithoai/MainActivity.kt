package vn.edu.ltm.goithoai

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Contacts
import androidx.compose.material.icons.filled.DirectionsCar
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.MyLocation
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import vn.edu.ltm.goithoai.ui.GiaoDienGoiThoai
import vn.edu.ltm.goithoai.ui.ManHinhCuocGoi
import vn.edu.ltm.goithoai.ui.ManHinhDanhBa
import vn.edu.ltm.goithoai.ui.ManHinhKetNoi
import vn.edu.ltm.goithoai.ui.ManHinhKhach
import vn.edu.ltm.goithoai.ui.ManHinhLichSu
import vn.edu.ltm.goithoai.ui.ManHinhTaiXe
import vn.edu.ltm.goithoai.ui.Mau
import vn.edu.ltm.goithoai.ui.kieu

/**
 * Bài mở rộng MR6.15 – app Android "Gọi thoại LTM": gọi thoại qua máy chủ trung gian (WebRTC) và theo dõi vị trí
 * tài xế (MQTT), giống tính năng gọi ẩn số và bản đồ chuyến đi của các ứng dụng gọi xe.
 * Một Activity duy nhất; màn hình chọn theo state của UngDungViewModel.
 */
class MainActivity : ComponentActivity() {
    private val vm: UngDungViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
        )
        // Giữ màn hình sáng khi app mở: bản lab không chạy dịch vụ nền, tắt màn hình thì không nhận được cuộc gọi.
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContent { GiaoDienGoiThoai { UngDung(vm) } }
    }

    @Composable
    private fun UngDung(vm: UngDungViewModel) {
        val view = LocalView.current
        val toi = vm.cuoc != null
        SideEffect {                                            // chữ thanh trạng thái sáng trên nền tối khi gọi
            val ic = WindowCompat.getInsetsController(window, view)
            ic.isAppearanceLightStatusBars = !toi
            ic.isAppearanceLightNavigationBars = !toi
        }

        val xinMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { duoc ->
            if (!duoc) vm.thongBao = "Chưa cấp quyền micro: bên kia sẽ không nghe thấy bạn"
            vm.ketNoiMayChu()
        }
        val xinViTri = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { kq ->
            if (kq[Manifest.permission.ACCESS_FINE_LOCATION] == true) vm.taiXe.batGps()
            else vm.thongBao = "Chưa cấp quyền vị trí chính xác: dùng chế độ Giả lập"
        }
        val chonGpx = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) napGpx(uri)
        }

        val c = vm.cuoc
        when {
            vm.ketNoi != "da" -> ManHinhKetNoi(vm.caiDat, vm.ketNoi == "dang", vm.loiKetNoi, vm::capNhatCaiDat) {
                xinMic.launch(Manifest.permission.RECORD_AUDIO)
            }
            c != null -> {
                BackHandler { }                                 // không thoát giữa cuộc gọi bằng nút Back
                ManHinhCuocGoi(c, vm::cupMay, vm::traLoi, vm::tuChoi, vm::doiTatMic, vm::doiLoaNgoai, vm::doiGiuMay)
            }
            else -> {
                BackHandler(enabled = vm.tab != 0) { vm.tab = 0 }
                val snack = remember { SnackbarHostState() }
                LaunchedEffect(vm.thongBao) {
                    vm.thongBao?.let {
                        snack.showSnackbar(it)
                        vm.thongBao = null
                    }
                }
                val laTaiXe = vm.caiDat.vaiTro == "tai_xe"
                val mayChu = "${vm.caiDat.host}:${vm.caiDat.cong}" + if (vm.caiDat.tls) " · TLS" else ""
                Scaffold(
                    containerColor = Mau.nen,
                    snackbarHost = { SnackbarHost(snack) },
                    bottomBar = {
                        NavigationBar(containerColor = Mau.trang) {
                            val mau = NavigationBarItemDefaults.colors(
                                selectedIconColor = Mau.xanhDam, selectedTextColor = Mau.xanhDam,
                                indicatorColor = Mau.xanhNhat, unselectedIconColor = Mau.chuPhu,
                                unselectedTextColor = Mau.chuPhu,
                            )
                            NavigationBarItem(vm.tab == 0, { vm.tab = 0 }, { Icon(Icons.Filled.Contacts, null) },
                                label = { Text("Danh bạ", style = kieu(12)) }, colors = mau)
                            NavigationBarItem(vm.tab == 1, { vm.tab = 1; vm.taiLichSu() }, { Icon(Icons.Filled.History, null) },
                                label = { Text("Lịch sử", style = kieu(12)) }, colors = mau)
                            NavigationBarItem(vm.tab == 2, { vm.tab = 2 },
                                { Icon(if (laTaiXe) Icons.Filled.MyLocation else Icons.Filled.DirectionsCar, null) },
                                label = { Text(if (laTaiXe) "Vị trí" else "Chuyến đi", style = kieu(12)) }, colors = mau)
                        }
                    },
                ) { dem ->
                    Box(Modifier.fillMaxSize().background(Mau.nen).padding(dem)) {
                        when (vm.tab) {
                            0 -> ManHinhDanhBa(vm.danhBa, mayChu, vm::ngatKetNoi, vm::goi)
                            1 -> ManHinhLichSu(vm.lichSu, vm.caiDat.ma, mayChu, vm.trinhPhat, vm::ngatKetNoi, vm::taiLichSu,
                                vm::phat, vm::doiPhatTamDung, vm::tua) { moNgoai(vm.urlBanGhi(it)) }
                            else -> if (laTaiXe) ManHinhTaiXe(vm.taiXe, mayChu, vm::ngatKetNoi,
                                xinQuyenGps = {
                                    xinViTri.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION,
                                        Manifest.permission.ACCESS_COARSE_LOCATION))
                                },
                                chonGpx = { chonGpx.launch(arrayOf("*/*")) }, goiKhach = vm::goiKhach)
                            else ManHinhKhach(vm.theoDoi, vm.danhBa, mayChu, vm.caiDat.diemDon, vm::ngatKetNoi,
                                vm::theoDoiXe, vm::goiTaiXeTheoDoi) { chiaSeChuyen() }
                        }
                    }
                }
            }
        }
    }

    private fun napGpx(uri: Uri) {
        try {
            var ten = "tuyen.gpx"
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) ten = it.getString(0) ?: ten
            }
            val noiDung = contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() } ?: ""
            vm.taiXe.napGpx(ten, noiDung)?.let { vm.thongBao = it }
        } catch (e: Exception) {
            vm.thongBao = "Không đọc được tệp: ${e.message}"
        }
    }

    private fun chiaSeChuyen() {
        val v = vm.theoDoi.viTri
        val s = "Mình đang theo dõi chuyến xe ${vm.theoDoi.maXe}" +
            (v?.let { " – vị trí tài xế: https://www.openstreetmap.org/?mlat=${it.lat}&mlon=${it.lon}#map=17/${it.lat}/${it.lon}" } ?: "")
        startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, s),
            "Chia sẻ chuyến"))
    }

    /** Mở bản ghi bằng ứng dụng khác (trình duyệt phát được Ogg/Opus) khi MediaPlayer của máy không phát được. */
    private fun moNgoai(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (e: ActivityNotFoundException) {
            vm.thongBao = "Không có ứng dụng mở được $url"
        }
    }
}
