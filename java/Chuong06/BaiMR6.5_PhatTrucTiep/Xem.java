import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Người xem phát trực tiếp (bản dòng lệnh).
 *
 * Lõi ở NguoiXem.java. Mỗi giây in: số gói tiếng nhận, tổng gói mất, trễ đầu-cuối TB, số khung hiện và lệch
 * tiếng-hình TB trong giây đó; cuối cùng in TỔNG KẾT. Không phát ra loa (bản giao diện XemGui mới phát loa nếu máy có
 * thiết bị phát) – đồng hồ phát vẫn chạy đúng nhịp 20 ms.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
 *   chup <nhãn>   dong_bo bat|tat   dang_ky am_thanh|video|ca_hai   thoat
 *
 * Chạy:  java -cp out Xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5] [--khong-dong-bo]
 *            [--chi-am-thanh] [--chi-video] [--mat-goi 0.0] [--ghep ket_qua_video.png]
 *   --thoi-gian  tổng thời gian chạy (giây, tính cả thời gian chờ --tre)
 *   --tre        chờ bao nhiêu giây rồi mới vào xem (người vào muộn)
 *   --mat-goi    giả lập tỉ lệ mất gói ở chiều nhận (cả tiếng và hình)
 */
public class Xem {
    public static void main(String[] args) throws Exception {
        double t0 = Chung.bayGio();
        if (args.length < 3) {
            System.err.println("Cách dùng: java -cp out Xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5]"
                    + " [--khong-dong-bo] [--chi-am-thanh] [--chi-video] [--mat-goi 0.0] [--ghep tệp.png]");
            System.exit(2);
        }
        String ten = args[0], host = args[1], ghep = null;
        int cong = Integer.parseInt(args[2]), dem = 5;
        double thoiGian = 10.0, tre = 0.0, matGoi = 0.0;
        boolean khongDongBo = false, chiAm = false, chiVideo = false;
        for (int i = 3; i < args.length; i++) {
            switch (args[i]) {
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--tre" -> tre = Double.parseDouble(args[++i]);
                case "--dem" -> dem = Integer.parseInt(args[++i]);
                case "--mat-goi" -> matGoi = Double.parseDouble(args[++i]);
                case "--ghep" -> ghep = args[++i];
                case "--khong-dong-bo" -> khongDongBo = true;
                case "--chi-am-thanh" -> chiAm = true;
                case "--chi-video" -> chiVideo = true;
                default -> throw new IllegalArgumentException("tùy chọn lạ: " + args[i]);
            }
        }
        Consumer<String> log = Chung.nguoiIn("[xem " + ten + "] ");
        NguoiXem nx = new NguoiXem(ten, log, Math.max(1, Math.min(25, dem)), !khongDongBo, !chiVideo, !chiAm, matGoi, null);
        List<Chung.AnhNhan> daChup = new ArrayList<>();
        AtomicBoolean xong = new AtomicBoolean();
        CountDownLatch daIn = new CountDownLatch(1);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {        // Ctrl+C: vẫn in tổng kết
            xong.set(true);
            try { daIn.await(4, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
        }));

        Consumer<String> lenh = dong -> {
            String[] p = dong.split("\\s+");
            log.accept("> " + dong);
            if (p[0].equals("dong_bo") && p.length == 2 && (p[1].equals("bat") || p[1].equals("tat"))) {
                nx.datDongBo(p[1].equals("bat"));
            } else if (p[0].equals("dang_ky") && p.length == 2 && List.of("am_thanh", "video", "ca_hai").contains(p[1])) {
                nx.dangKy(!p[1].equals("video"), !p[1].equals("am_thanh"));
            } else if (p[0].equals("chup")) {
                NguoiXem.Chup c = nx.chup();
                if (c == null) {
                    log.accept("chưa có hình để chụp");
                    return;
                }
                NguoiXem.AnhRgb a = NguoiXem.anhChup(c);
                String duong = "out/" + ten + "_chup_" + (daChup.size() + 1) + ".png";
                try {
                    Chung.ghiPng(duong, a.rgb(), a.w(), a.h());
                } catch (IOException e) {
                    log.accept("Không ghi được " + duong + ": " + e.getMessage());
                    return;
                }
                String nhan = dong.contains(" ") ? String.join(" ", java.util.Arrays.copyOfRange(p, 1, p.length)) : duong;
                daChup.add(new Chung.AnhNhan(a.w(), a.h(), a.rgb(), nhan));
                log.accept(Chung.f("Đã chụp %s: hình %.3f s, tiếng ", duong, c.ts() / 1000.0)
                        + (c.am() == null ? "–" : Chung.f("%.3f s", c.am() / 1000)));
            } else if (p[0].equals("thoat")) {
                xong.set(true);
            } else {
                log.accept("lệnh không hợp lệ");
            }
        };
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, lenh, null));
        if (tre > 0) {
            log.accept(Chung.f("Chờ %.1f s rồi mới vào xem (vào muộn)", tre));
            Chung.ngu(tre);
        }
        try {
            nx.ketNoi(host, cong);
        } catch (IOException e) {
            log.accept("Không kết nối được: " + e.getMessage());
            daIn.countDown();
            System.exit(0);
        }
        String nhan = nx.amThanh && nx.video ? "tiếng+hình" : nx.amThanh ? "tiếng" : "hình";
        log.accept("Nhận " + nhan + ", đồng bộ " + (nx.dongBo ? "BẬT" : "TẮT") + ", jitter " + nx.doSau * 20 + " ms"
                + (matGoi > 0 ? ", mất " + Chung.f("%s", gonG(matGoi * 100)) + "%" : ""));
        log.accept("Mỗi giây: gói tiếng, mất, trễ ms | khung hiện, lệch ms");
        int giay = 0;
        double moc = Chung.bayGio();
        while (Chung.bayGio() - t0 < thoiGian && !xong.get() && nx.daKetNoi) {
            if (Chung.bayGio() - moc >= 1.0) {
                moc += 1.0;
                giay++;
                NguoiXem.GiayQua r = nx.thongKeGiay();
                log.accept(Chung.f("%2ds tiếng %2d mất %d trễ %3s | hình %2d lệch %4s", giay, r.am(), r.mat(),
                        NguoiXem.chuMs(r.tre(), false), r.hien(), NguoiXem.chuMs(r.lech(), true)));
            }
            Chung.ngu(0.02);
        }
        NguoiXem.TrangThai tt = nx.trangThai();
        nx.dong();
        for (String d : NguoiXem.dongTongKet(tt, tre)) log.accept(d);
        if (matGoi > 0) log.accept("giả lập mất " + tt.boGiaLap + " gói nhận");
        if (ghep != null && !daChup.isEmpty()) {
            int[] wh = Chung.ghepAnh(ghep, daChup, 2);
            log.accept("Đã ghép " + daChup.size() + " ảnh vào " + ghep + " (" + wh[0] + "x" + wh[1] + ")");
        }
        daIn.countDown();
        System.exit(0);
    }

    /** Như định dạng {:g} của Python cho số phần trăm: 3.0 -> "3", 2.5 -> "2.5". */
    static String gonG(double x) {
        String s = Chung.f("%.6g", x);
        if (s.contains("e")) return s;
        if (s.contains(".")) s = s.replaceAll("0+$", "").replaceAll("\\.$", "");
        return s;
    }
}
