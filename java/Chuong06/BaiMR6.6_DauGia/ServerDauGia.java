import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR6.6 – Máy chủ đấu giá (bản dòng lệnh).
 *
 * Lõi ở MayChu.java (MayChuDauGia, khung MayChuCoSo trong Chung.java, dùng chung với ServerGui); tệp này chỉ in nhật
 * ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
 *   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
 *   (tham số đặt lại: ten_mon, gia_khoi_diem, buoc_gia, thoi_gian_s (0 = không giới hạn), gia_han_s)
 *
 * Chạy:  java -cp out ServerDauGia [host] [cổng] [--che-do khoa] [--tre-ms 2] [--thoi-gian 0] [--gia 5000000]
 *                                  [--buoc 100000] [--gia-han 10]
 * Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1). Ctrl+C vẫn in bảng tổng kết.
 */
public class ServerDauGia {
    static void log(String text) { Chung.inDong("server", text); }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        String cheDo = "khoa";
        double treMs = 2.0, thoiGian = 0.0, giaHan = 10.0;
        long gia = 5_000_000, buoc = 100_000;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--che-do" -> cheDo = args[++i];
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--gia" -> gia = Long.parseLong(args[++i]);
                case "--buoc" -> buoc = Long.parseLong(args[++i]);
                case "--gia-han" -> giaHan = Double.parseDouble(args[++i]);
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43700;
        MayChu.MayChuDauGia sv = new MayChu.MayChuDauGia(cheDo, treMs, ServerDauGia::log,
                Json.obj("thoi_gian_s", thoiGian, "gia_khoi_diem", gia, "buoc_gia", buoc, "gia_han_s", giaHan));
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
