import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR6.12 – Máy chủ ngân hàng (bản dòng lệnh).
 *
 * Lõi ở MayChu.java (MayChuNganHang, khung MayChuCoSo trong Chung.java, dùng chung với ServerGui); tệp này chỉ in nhật
 * ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
 *   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
 *   (tham số đặt lại: so_tk (5–20), so_du_dau, khoa_ngay_tho (0 = khóa theo thứ tự mã, 1 = nguồn rồi đích))
 *
 * Chạy:  java -cp out ServerNganHang [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-tk 10] [--so-du 5000000]
 *                                  [--khoa-ngay-tho]
 * Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1). Ctrl+C vẫn in bảng tổng kết.
 */
public class ServerNganHang {
    static void log(String text) { Chung.inDong("server", text); }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        String cheDo = "khoa";
        double treMs = 2.0;
        long soTk = 10, soDu = 5_000_000;
        boolean ngayTho = false;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--che-do" -> cheDo = args[++i];
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                case "--so-tk" -> soTk = Long.parseLong(args[++i]);
                case "--so-du" -> soDu = Long.parseLong(args[++i]);
                case "--khoa-ngay-tho" -> ngayTho = true;
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43760;
        MayChu.MayChuNganHang sv = new MayChu.MayChuNganHang(cheDo, treMs, ServerNganHang::log,
                Json.obj("so_tk", soTk, "so_du_dau", soDu, "khoa_ngay_tho", ngayTho ? 1L : 0L));
        sv.batDau(host, cong);
        if (ngayTho) log("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)");
        CountDownLatch xong = new CountDownLatch(1);
        AtomicBoolean daKet = new AtomicBoolean();
        Runnable ketThuc = () -> {
            if (!daKet.compareAndSet(false, true)) return;
            log("Tổng kết (từ lần đặt lại cuối):");
            for (String d : Chung.dongThongKe(sv.thongKeDayDu())) log("  " + d);
            sv.dung();
        };
        Runtime.getRuntime().addShutdownHook(new Thread(ketThuc));     // Ctrl+C (công cụ demo dừng bằng SIGINT)
        double t0 = Chung.bayGio();
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, dong -> {
            String[] p = dong.split("\\s+");
            log("> " + dong);
            try {
                if (p[0].equals("che_do") && p.length == 2) sv.datCheDo(p[1]);
                else if (p[0].equals("tre") && p.length == 2) sv.datTre(Double.parseDouble(p[1]));
                else if (p[0].equals("dat_lai")) sv.datLai(Chung.docThamSo(Arrays.asList(p).subList(1, p.length)));
                else if (p[0].equals("thong_ke")) for (String d : Chung.dongThongKe(sv.thongKeDayDu())) log(d);
                else if (p[0].equals("thoat")) xong.countDown();
                else log("lệnh không hợp lệ");
            } catch (IllegalArgumentException e) {
                log("lỗi: " + e.getMessage());
            }
        }, null));
        long daIn = 0;
        while (!xong.await(5, TimeUnit.SECONDS)) {                  // mỗi 5 s: một dòng số liệu nếu có yêu cầu mới
            var tk = sv.thongKeDayDu();
            if (Chung.so(tk.get("so_yc")) != daIn) {
                daIn = Chung.so(tk.get("so_yc"));
                log(tk.get("che_do") + ": " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", từ chối "
                        + tk.get("tu_choi") + ", sai phạm " + tk.get("sai_pham_tong"));
            }
        }
        ketThuc.run();
        System.exit(0);
    }
}
