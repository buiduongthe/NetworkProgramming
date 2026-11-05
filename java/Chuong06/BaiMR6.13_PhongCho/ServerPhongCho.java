import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR6.13 – Máy chủ bán vé có phòng chờ ảo (bản dòng lệnh).
 *
 * Lõi ở MayChu.java (MayChuPhongCho, khung MayChuCoSo trong Chung.java, dùng chung với ServerGui); tệp này chỉ in nhật
 * ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
 *   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
 *   toc_do <người/s>  (đổi tốc độ cấp lượt ngay, không đặt lại)
 *   (tham số đặt lại: tong_ve, ve_toi_da, gia_ve, ten_su_kien, phong_cho (0/1), toc_do, dung_luong,
 *    luot_s, gioi_han_xu_ly)
 *
 * Chạy:  java -cp out ServerPhongCho [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
 *                                  [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
 * Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1). Ctrl+C vẫn in bảng tổng kết.
 */
public class ServerPhongCho {
    static void log(String text) { Chung.inDong("server", text); }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        String cheDo = "khoa";
        double treMs = 2.0;
        long tongVe = 500, dungLuong = 50, gioiHan = 64;
        double tocDo = 50.0, luotS = 20.0;
        boolean tat = false;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--che-do" -> cheDo = args[++i];
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                case "--tong-ve" -> tongVe = Long.parseLong(args[++i]);
                case "--toc-do" -> tocDo = Double.parseDouble(args[++i]);
                case "--dung-luong" -> dungLuong = Long.parseLong(args[++i]);
                case "--luot-s" -> luotS = Double.parseDouble(args[++i]);
                case "--gioi-han" -> gioiHan = Long.parseLong(args[++i]);
                case "--tat-phong-cho" -> tat = true;
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43770;
        MayChu.MayChuPhongCho sv = new MayChu.MayChuPhongCho(cheDo, treMs, ServerPhongCho::log,
                Json.obj("tong_ve", tongVe, "toc_do", tocDo, "dung_luong", dungLuong,
                        "luot_s", luotS, "gioi_han_xu_ly", gioiHan, "phong_cho", tat ? 0L : 1L));
        sv.batDau(host, cong);
        log("Phòng chờ " + (tat ? "TẮT" : "BẬT") + ": " + tongVe + " vé, cấp lượt " + Chung.g(tocDo) + " người/s");
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
                else if (p[0].equals("toc_do") && p.length == 2) sv.datTocDo(Double.parseDouble(p[1]));
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
