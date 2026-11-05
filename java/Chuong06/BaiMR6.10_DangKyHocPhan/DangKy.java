import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.10 – Sinh viên đăng ký học phần (bản dòng lệnh).
 *
 * Kết nối máy chủ, in bảng lớp (mã lớp, tên, đã đăng ký/sĩ số) và nhận lệnh:
 *   dk <mã lớp>       đăng ký lớp            dk <mã lớp> x2    "bấm đúp": gửi HAI yêu cầu liên tiếp không chờ
 *   huy <mã lớp>      hủy lớp đã đăng ký     xem               in lại bảng lớp            thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out DangKy <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class DangKy {
    final String ten;
    volatile Map<String, Object> tt;                 // cap_nhat mới nhất
    final KetNoiKhach kn;

    DangKy(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        tt = kn.trangThai;
        log("Vào cổng đăng ký (" + tt.get("che_do") + "), sĩ số " + tt.get("si_so"));
        xem();
        kn.batDauDoc();
    }

    void xem() {
        for (Object o : Chung.list(tt.get("lop"))) {
            Map<String, Object> x = Chung.map(o);
            String cuaToi = Chung.list(x.get("sv")).contains(ten) ? " *" : "";
            log(Chung.f("  %s %-15s %3d/%s%s", x.get("ma"), x.get("ten"), Chung.so(x.get("da_dk")), x.get("si_so"), cuaToi));
        }
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) { tt = moi; }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void gui(String loai, String ma) throws IOException {
        kn.guiYc(loai, (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                String viec = loai.equals("dang_ky") ? "ĐĂNG KÝ" : "HỦY";
                log("  " + viec + " " + ma + " thành công (" + tin.get("da_dk") + "/" + tt.get("si_so") + Chung.f(", %.1f ms)", rtt));
            } else log("  " + ma + ": TỪ CHỐI – " + tin.get("ly_do"));
        }, Json.obj("ma_lop", ma));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p[0].equals("dk") && p.length == 2) {
                log("> đăng ký " + p[1]);
                gui("dang_ky", p[1]);
            } else if (p[0].equals("dk") && p.length == 3 && p[2].equals("x2")) {
                log("> đăng ký " + p[1] + " (bấm đúp: 2 yêu cầu)");
                gui("dang_ky", p[1]);
                gui("dang_ky", p[1]);
            } else if (p[0].equals("huy") && p.length == 2) {
                log("> hủy " + p[1]);
                gui("huy", p[1]);
            } else if (p[0].equals("xem")) {
                xem();
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: dk <mã> [x2] | huy <mã> | xem | thoat");
        } catch (IOException e) {
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43740;
        double t0 = Chung.bayGio();
        DangKy sv = new DangKy(vt.get(0));
        sv.ketNoi(host, cong);
        CountDownLatch hetStdin = new CountDownLatch(1);
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, sv::lenh, hetStdin::countDown));
        if (thoiGian > 0) {
            while (sv.kn.mo && Chung.bayGio() - t0 < thoiGian) Thread.sleep(100);
        } else {
            while (sv.kn.mo && !hetStdin.await(100, TimeUnit.MILLISECONDS)) { }
            Thread.sleep(500);                                  // chờ trả lời của lệnh cuối
        }
        sv.kn.khiDong = () -> { };
        sv.kn.dong();
        System.exit(0);
    }
}
