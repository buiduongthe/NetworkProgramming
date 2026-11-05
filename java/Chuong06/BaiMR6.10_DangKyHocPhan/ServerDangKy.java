import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR6.10 – Máy chủ đăng ký học phần (bản dòng lệnh).
 *
 * Lõi ở MayChu.java (MayChuDangKy, khung MayChuCoSo trong Chung.java, dùng chung với ServerGui); tệp này chỉ in nhật
 * ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
 *   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
 *   (tham số đặt lại: so_lop (1–6), si_so (1–500))
 *
 * Chạy:  java -cp out ServerDangKy [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-lop 3] [--si-so 50]
 * Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1). Ctrl+C vẫn in bảng tổng kết.
 */
public class ServerDangKy {
    static void log(String text) { Chung.inDong("server", text); }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        String cheDo = "khoa";
        double treMs = 2.0;
        long soLop = 3, siSo = 50;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--che-do" -> cheDo = args[++i];
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                case "--so-lop" -> soLop = Long.parseLong(args[++i]);
                case "--si-so" -> siSo = Long.parseLong(args[++i]);
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43740;
        MayChu.MayChuDangKy sv = new MayChu.MayChuDangKy(cheDo, treMs, ServerDangKy::log,
                Json.obj("so_lop", soLop, "si_so", siSo));
        sv.batDau(host, cong);
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
