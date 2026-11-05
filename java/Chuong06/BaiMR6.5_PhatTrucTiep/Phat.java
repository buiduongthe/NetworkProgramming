import java.io.File;
import java.io.IOException;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Máy phát trực tiếp (bản dòng lệnh): camera + micro -> máy chủ.
 *
 * Lõi ở MayPhat.java. Mặc định camera giả lập "bảng clapper" (đồng hồ, số khung, chớp trắng mỗi giây tròn) và
 * micro giả lập (440 Hz + bíp 1000 Hz mỗi giây tròn).
 *   --camera testsrc     ảnh mẫu thử của ffmpeg (cần ffmpeg)
 *   --camera <thiết bị>  camera thật qua ffmpeg (/dev/video0, "Tên camera" trên Windows, 0 trên macOS)
 *   --mic <chỉ số>       micro thật qua javax.sound (chỉ số trong danh sách micro của máy)
 *   --tep <tệp video>    phát lại hình + tiếng của tệp (mp4, mkv…) qua ffmpeg, lặp lại khi hết tệp;
 *                        thêm --mic gia_lap|<chỉ số> để thay tiếng của tệp bằng micro
 *   --tao-tep-mau        tạo trước tệp mẫu 10 s (chớp trắng + bíp mỗi giây) tại đường dẫn của --tep
 * Thiết bị thật không mở được -> tự lùi về nguồn giả lập (có báo).
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t):  khung_khoa   dung
 *
 * Chạy:  java -cp out Phat <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>]
 *            [--mic gia_lap|<chỉ số>] [--tep <tệp video> [--tao-tep-mau]] [--fps 10] [--gop 20]
 *            [--thoi-gian 30] [--mat-goi 0.0]
 */
public class Phat {
    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("Cách dùng: java -cp out Phat <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>]"
                    + " [--mic gia_lap|<chỉ số>] [--tep <tệp video> [--tao-tep-mau]] [--fps 10] [--gop 20] [--thoi-gian 30] [--mat-goi 0.0]");
            System.exit(2);
        }
        String ten = args[0], host = args[1], camera = Camera.GIA_LAP, micro = null, tep = null;
        boolean taoTepMau = false;
        int cong = Integer.parseInt(args[2]), fps = Chung.FPS_MAC_DINH, gop = Chung.GOP_MAC_DINH;
        double thoiGian = 30.0, matGoi = 0.0;
        for (int i = 3; i < args.length; i++) {
            switch (args[i]) {
                case "--camera" -> camera = args[++i];
                case "--mic" -> micro = args[++i];                 // mặc định: gia_lap (hoặc tiếng của tệp khi có --tep)
                case "--tep" -> tep = args[++i];
                case "--tao-tep-mau" -> taoTepMau = true;
                case "--fps" -> fps = Integer.parseInt(args[++i]);
                case "--gop" -> gop = Integer.parseInt(args[++i]);
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--mat-goi" -> matGoi = Double.parseDouble(args[++i]);
                default -> throw new IllegalArgumentException("tùy chọn lạ: " + args[i]);
            }
        }
        Consumer<String> log = Chung.nguoiIn("[phat " + ten + "] ");
        MayPhat mp = new MayPhat(ten, log, matGoi);
        int ma;
        try {
            ma = mp.ketNoi(host, cong);
        } catch (IOException e) {
            log.accept("Không kết nối được: " + e.getMessage());
            return;
        }
        Camera.NguonCamera cam = null;
        ThietBiAmThanh.NguonAm mic = null;
        if (tep != null) {
            try {
                if (taoTepMau) {
                    File cha = new File(tep).getAbsoluteFile().getParentFile();
                    if (cha != null) cha.mkdirs();
                    TepVideo.taoTepMau(tep, 10);
                    log.accept("Đã tạo tệp mẫu " + new File(tep).getName() + " (10 s)");
                }
                TepVideo.MoTep m = TepVideo.moTep(tep, fps, micro == null);
                cam = m.camera();
                mic = m.mic();
            } catch (IOException e) {
                log.accept("Không mở được tệp video: " + e.getMessage());
                cam = null;
                mic = null;
            }
        }
        if (cam == null) {
            if (!camera.equals(Camera.GIA_LAP) && Camera.coFfmpeg()) {
                cam = new Camera.QuaFfmpeg(camera, camera, fps);
            } else {
                if (!camera.equals(Camera.GIA_LAP)) log.accept("Không có ffmpeg: dùng camera giả lập");
                cam = new Camera.GiaLap(ten, ma, fps);
            }
        }
        if (mic == null) {
            String tenMicro = micro == null ? "gia_lap" : micro, tenMixer = null;
            boolean loiMic = false;
            if (!tenMicro.equals("gia_lap")) {
                List<String> ds = ThietBiAmThanh.dsMicro();
                try {
                    tenMixer = ds.get(Integer.parseInt(tenMicro));
                } catch (RuntimeException e) {
                    loiMic = true;
                }
            }
            ThietBiAmThanh.KetQuaMic kq = ThietBiAmThanh.moMic(tenMixer);
            if (loiMic || !kq.loi().isEmpty()) log.accept("Không mở được micro thật: dùng giả lập");
            mic = kq.mic();
        }
        log.accept("Mã " + ma + "; camera " + cam.moTa());
        log.accept("Micro " + mic.moTa());

        AtomicBoolean xong = new AtomicBoolean();
        CountDownLatch daIn = new CountDownLatch(1);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {        // Ctrl+C: vẫn in tổng kết
            xong.set(true);
            try { daIn.await(4, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
        }));
        Consumer<String> lenh = dong -> {
            log.accept("> " + dong);
            if (dong.equals("khung_khoa")) mp.khungKhoaNgay("lệnh khung_khoa");
            else if (dong.equals("dung")) xong.set(true);
            else log.accept("lệnh không hợp lệ");
        };
        double t0 = Chung.bayGio();
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, lenh, null));
        mp.batDauPhat(cam, mic, fps, gop);
        log.accept("Mỗi giây: khung I/P, kbit/s hình, tiếng (cả header)");
        int giay = 0;
        double moc = Chung.bayGio();
        while (Chung.bayGio() - t0 < thoiGian && !xong.get() && mp.daKetNoi) {
            if (Chung.bayGio() - moc >= 1.0) {
                moc += 1.0;
                giay++;
                MayPhat.GiayQua r = mp.thongKeGiay();
                log.accept(Chung.f("%2ds khung I %d P %2d | hình %4.0f tiếng %3.0f kbit/s", giay, r.i(), r.p(),
                        r.kbpsHinh(), r.kbpsAm()));
            }
            Chung.ngu(0.02);
        }
        double tong = Chung.bayGio() - t0;
        MayPhat.TrangThai tt = mp.trangThai();
        mp.dong();
        for (String d : MayPhat.dongTongKet(tt, tong)) log.accept(d);
        if (matGoi > 0) log.accept("giả lập mất " + mp.boGiaLap + " gói gửi");
        daIn.countDown();
        System.exit(0);
    }
}
