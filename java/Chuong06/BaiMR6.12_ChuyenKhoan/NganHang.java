import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.12 – Khách hàng ngân hàng (bản dòng lệnh).
 *
 * Kết nối máy chủ, đăng nhập một tài khoản (mặc định tài khoản mang tên mình, ví dụ An -> TK01), in số dư, tiền đến
 * ("+ Nhận …") và kết quả chuyển khoản; nhận lệnh:
 *   chuyen <TK đích> <số tiền> [nội dung]     so_du     thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out NganHang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]   (0 = tới khi hết stdin/thoat)
 */
public class NganHang {
    final String ten;
    String tk;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    final Set<Object> daThay = new HashSet<>();      // ma_gd đã biết (không báo tiền đến hai lần)
    final Object khoa = new Object();
    final KetNoiKhach kn;

    NganHang(String ten, String tk) {
        this.ten = ten;
        this.tk = tk;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    long soDu(Map<String, Object> t) {
        for (Object o : Chung.list(t.get("tai_khoan"))) if (tk.equals(Chung.map(o).get("ma"))) return Chung.so(Chung.map(o).get("so_du"));
        throw new IllegalStateException("không có " + tk);
    }

    String chu(String ma) {
        for (Object o : Chung.list(tt.get("tai_khoan"))) if (ma.equals(Chung.map(o).get("ma"))) return String.valueOf(Chung.map(o).get("chu"));
        return "?";
    }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) {
            t = tt = kn.trangThai;
            List<Object> ds = Chung.list(t.get("tai_khoan"));
            if (tk == null) {                                   // tài khoản mang tên mình, không có thì TK01
                tk = String.valueOf(Chung.map(ds.get(0)).get("ma"));
                for (Object o : ds) if (ten.equals(Chung.map(o).get("chu"))) { tk = String.valueOf(Chung.map(o).get("ma")); break; }
            }
            for (Object g : Chung.list(t.get("lich_su"))) daThay.add(Chung.map(g).get("ma"));
        }
        log("Đăng nhập " + tk + " – " + chu(tk) + " (" + t.get("che_do") + ")");
        log("  số dư " + Chung.tien(soDu(t)));
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        List<Map<String, Object>> moiGd = new ArrayList<>();
        synchronized (khoa) {
            tt = moi;
            List<Object> ls = Chung.list(moi.get("lich_su"));
            for (int i = ls.size() - 1; i >= 0; i--) {
                Map<String, Object> g = Chung.map(ls.get(i));
                if (daThay.add(g.get("ma"))) moiGd.add(g);
            }
        }
        for (Map<String, Object> g : moiGd)
            if (tk.equals(g.get("den")))
                log("+ Nhận " + Chung.tien(Chung.so(g.get("so_tien"))) + " từ " + g.get("tu") + " (" + chu(String.valueOf(g.get("tu")))
                        + "), số dư " + Chung.tien(soDu(moi)));
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void chuyen(String den, long soTien, String noiDung) throws IOException {
        log("> chuyển " + Chung.tien(soTien) + " " + tk + "→" + den);
        kn.guiYc("chuyen", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) { daThay.add(tin.get("ma_gd")); }
                log(Chung.f("  THÀNH CÔNG (%.1f ms), số dư ", rtt) + Chung.tien(Chung.so(tin.get("so_du"))));
            } else log(Chung.f("  TỪ CHỐI (%.0f ms): ", rtt) + tin.get("ly_do"));
        }, Json.obj("tu", tk, "den", den, "so_tien", soTien, "noi_dung", noiDung));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p[0].equals("chuyen") && p.length >= 3)
                chuyen(p[1], Long.parseLong(p[2]), String.join(" ", Arrays.asList(p).subList(3, p.length)));
            else if (p[0].equals("so_du")) {
                synchronized (khoa) { log("Số dư " + tk + ": " + Chung.tien(soDu(tt))); }
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: chuyen <TK> <số tiền> [nội dung] | so_du | thoat");
        } catch (IllegalArgumentException | IOException e) {
            log("lỗi: " + e.getMessage());
        }
    }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        double thoiGian = 0;
        String tk = null;
        for (int i = 0; i < args.length; i++) {
            if (args[i].equals("--thoi-gian")) thoiGian = Double.parseDouble(args[++i]);
            else if (args[i].equals("--tk")) tk = args[++i];
            else vt.add(args[i]);
        }
        String host = vt.size() > 1 ? vt.get(1) : "127.0.0.1";
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43760;
        double t0 = Chung.bayGio();
        NganHang kh = new NganHang(vt.get(0), tk);
        kh.ketNoi(host, cong);
        CountDownLatch hetStdin = new CountDownLatch(1);
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, kh::lenh, hetStdin::countDown));
        if (thoiGian > 0) {                                     // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
            while (kh.kn.mo && Chung.bayGio() - t0 < thoiGian) Thread.sleep(100);
        } else {                                                // tới khi hết stdin / thoat
            while (kh.kn.mo && !hetStdin.await(100, TimeUnit.MILLISECONDS)) { }
            Thread.sleep(300);                                  // chờ trả lời của lệnh cuối
        }
        kh.kn.khiDong = () -> { };
        kh.kn.dong();
        System.exit(0);
    }
}
