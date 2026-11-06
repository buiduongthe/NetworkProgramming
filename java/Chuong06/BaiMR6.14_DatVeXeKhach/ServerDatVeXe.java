import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR6.14 – Máy chủ đặt vé xe khách theo chặng (bản dòng lệnh).
 *
 * Lõi ở MayChu.java (MayChuDatVeXe, khung MayChuCoSo trong Chung.java, dùng chung với ServerGui); tệp này chỉ in nhật
 * ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
 *   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
 *   (tham số đặt lại: ttl_s – thời hạn giữ chỗ, loai_xe – giuong | ghe, ghe_nong – chỗ mọi người ảo nhắm
 *    tới đầu tiên (rỗng = chọn ngẫu nhiên), chuyen – giờ chạy, nha_xe)
 *
 * Chạy:  java -cp out ServerDatVeXe [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ttl 30] [--loai-xe giuong]
 *                                    [--ghe-nong D01]
 * Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1). Ctrl+C vẫn in bảng tổng kết.
 */
public class ServerDatVeXe {
    static void log(String text) { Chung.inDong("server", text); }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        String cheDo = "khoa";
        double treMs = 2.0, ttl = 30.0;
        String gheNong = null, loaiXe = "giuong";
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--che-do" -> cheDo = args[++i];
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                case "--ttl" -> ttl = Double.parseDouble(args[++i]);
                case "--loai-xe" -> loaiXe = args[++i];
                case "--ghe-nong" -> gheNong = args[++i];
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43780;
        var ts = Json.obj("ttl_s", ttl, "loai_xe", loaiXe);
        if (gheNong != null) ts.put("ghe_nong", gheNong.toUpperCase());   // mặc định: chỗ đầu của loại xe (D01 / 01)
        MayChu.MayChuDatVeXe sv = new MayChu.MayChuDatVeXe(cheDo, treMs, ServerDatVeXe::log, ts);
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
