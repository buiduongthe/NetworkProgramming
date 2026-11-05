import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.7 – Người gọi xe (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến (số xe trống, xe của mình bị "nhận khách khác") và nhận lệnh:
 *   dat            đặt xe từ điểm đón mặc định của mình (theo tên) tới điểm đến mặc định
 *   huy            hủy chuyến đã đặt                       thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out DatXe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class DatXe {
    /** Điểm đón và điểm đến cố định theo tên (để mỗi người một chỗ đứng khác nhau). */
    static double[][] diemMacDinh(String ten) {
        int s = ten.codePoints().sum();
        double[] don = MayChu.NGA_TU.get((s * 7) % MayChu.NGA_TU.size());
        double[] den = MayChu.NGA_TU.get((s * 11 + 9) % MayChu.NGA_TU.size());
        if (MayChu.km(don, den) < 5)                                   // quá gần: chọn góc xa nhất
            den = new double[] {don[0] < 0.5 ? 0.9 : 0.1, don[1] < 0.5 ? 0.9 : 0.1};
        return new double[][] {don, den};
    }

    final String ten;
    final double[] don, den;
    volatile Map<String, Object> tt;                 // cap_nhat mới nhất
    volatile Long xeToi;                             // mã xe mình được báo đặt thành công
    volatile long phienToi;                          // số thứ tự lần ghi (kq) của chuyến mình đặt
    volatile boolean dangHuy;
    final Object khoa = new Object();
    final KetNoiKhach kn;

    DatXe(String ten) {
        this.ten = ten;
        double[][] d = diemMacDinh(ten);
        don = d[0];
        den = d[1];
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t = tt = kn.trangThai;
        log("Mở ứng dụng (" + t.get("che_do") + "): " + t.get("trong") + "/" + t.get("so_xe") + " xe trống");
        log("  đón: " + MayChu.tenDiem(don));
        log("  đến: " + MayChu.tenDiem(den));
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        Map<String, Object> cu;
        synchronized (khoa) { cu = tt; tt = moi; }
        if (Chung.so(moi.get("trong")) != Chung.so(cu.get("trong"))) log("Xe trống: " + moi.get("trong") + "/" + moi.get("so_xe"));
        Long x = xeToi;
        if (x != null && !dangHuy && Chung.so(moi.get("phien")) >= phienToi) {   // bỏ qua ảnh chụp trước lần ghi của mình
            Map<String, Object> xe = null;
            for (Object o : Chung.list(moi.get("xe"))) if (Chung.so(Chung.map(o).get("ma")) == x) xe = Chung.map(o);
            Object khach = xe == null ? null : xe.get("khach");
            if (xe != null && khach != null && !ten.equals(khach)) {
                log("  Tài xế " + xe.get("tai_xe") + " đã nhận khách khác (" + khach + ")!");
                xeToi = null;
            } else if (xe == null || khach == null) {
                log("  Chuyến đã kết thúc (máy chủ đặt lại)");
                xeToi = null;
            }
        }
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void dat() throws IOException {
        log("> Đặt xe – đang tìm tài xế… (dự kiến " + Chung.tien(MayChu.cuoc(don, den)) + ")");
        kn.guiYc("dat_xe", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                phienToi = Chung.so(tin.get("phien"));
                xeToi = Chung.so(tin.get("xe"));
                log("  ĐẶT THÀNH CÔNG – tài xế " + tin.get("tai_xe") + Chung.f(" (%.1f ms)", rtt));
                log("  " + tin.get("bien_so") + ", xe " + tin.get("mau") + ", đến sau " + tin.get("phut") + " phút, cước "
                        + Chung.tien(Chung.so(tin.get("cuoc"))));
            } else if ("het_cho".equals(tin.get("ma_loi"))) log("  HẾT CHỖ: " + tin.get("ly_do"));
            else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("don", MayChu.dsDiem(don), "den", MayChu.dsDiem(den)));
    }

    void huy() throws IOException {
        log("> Hủy chuyến");
        dangHuy = true;
        kn.guiYc("huy", (tin, rtt) -> {
            dangHuy = false;
            if (Chung.dung(tin.get("ok"))) {
                xeToi = null;
                log("  Đã hủy, xe " + tin.get("xe") + " (" + tin.get("tai_xe") + ") trống");
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, null);
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p.length == 1 && p[0].equals("dat")) dat();
            else if (p.length == 1 && p[0].equals("huy")) huy();
            else if (p.length == 1 && p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: dat | huy | thoat");
        } catch (IllegalArgumentException | IOException e) {
            log("lỗi: " + e.getMessage());
        }
    }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        double thoiGian = 0;
        for (int i = 0; i < args.length; i++) {
            if (args[i].equals("--thoi-gian")) thoiGian = Double.parseDouble(args[++i]);
            else vt.add(args[i]);
        }
        String host = vt.size() > 1 ? vt.get(1) : "127.0.0.1";
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43710;
        double t0 = Chung.bayGio();
        DatXe ng = new DatXe(vt.get(0));
        ng.ketNoi(host, cong);
        CountDownLatch hetStdin = new CountDownLatch(1);
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, ng::lenh, hetStdin::countDown));
        if (thoiGian > 0) {
            while (ng.kn.mo && Chung.bayGio() - t0 < thoiGian) Thread.sleep(100);
        } else {
            while (ng.kn.mo && !hetStdin.await(100, TimeUnit.MILLISECONDS)) { }
            Thread.sleep(300);                                  // chờ trả lời của lệnh cuối
        }
        ng.kn.khiDong = () -> { };
        ng.kn.dong();
        System.exit(0);
    }
}
