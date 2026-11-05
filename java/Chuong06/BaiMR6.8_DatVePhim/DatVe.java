import java.io.IOException;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.8 – Người đặt vé xem phim (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến (ghế mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
 *   giu E7 [E8 …]   giữ chỗ (có thời hạn)          tt [E7 …]    thanh toán (mặc định: các ghế vừa giữ)
 *   nha [E7 …]      nhả ghế đang giữ               xem          in sơ đồ ghế          thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out DatVe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class DatVe {
    /** Các dòng sơ đồ ghế: . trống, g người khác giữ, b đã bán, G mình giữ, V vé của mình. */
    static List<String> soDoChu(Map<String, Object> tt, String ten) {
        List<String> ds = new ArrayList<>();
        ds.add("      [ MÀN HÌNH ]");
        int cot = (int) Chung.so(tt.get("cot"));
        StringBuilder so = new StringBuilder("  ");
        for (int c = 1; c <= cot; c++) so.append(' ').append(c % 10);
        ds.add(so.toString());
        String hang = String.valueOf(tt.get("hang"));
        List<Object> soDo = Chung.list(tt.get("so_do"));
        Map<String, Object> chu = Chung.map(tt.get("chu"));
        for (int r = 0; r < hang.length(); r++) {
            String dong = String.valueOf(soDo.get(r));
            List<String> o = new ArrayList<>();
            for (int c = 0; c < dong.length(); c++) {
                char x = dong.charAt(c);
                boolean cuaToi = ten.equals(chu.get("" + hang.charAt(r) + (c + 1)));
                o.add(cuaToi ? (x == 'g' ? "G" : x == 'b' ? "V" : "" + x) : "" + x);
            }
            ds.add(hang.charAt(r) + "  " + String.join(" ", o));
        }
        return ds;
    }

    final String ten;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    List<String> gioHang = new ArrayList<>();        // các ghế vừa giữ được (để "tt" không cần gõ lại)
    long phienGio;                                   // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    Set<String> daBao = new HashSet<>();             // ghế trong giỏ đã báo hết hạn / bị lấy
    final Object khoa = new Object();
    final KetNoiKhach kn;

    DatVe(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) { t = tt = kn.trangThai; }
        Map<String, Object> d = Chung.map(t.get("dem"));
        log("Vào rạp: " + t.get("phim") + ", suất " + t.get("suat") + " (" + t.get("che_do") + ")");
        log("  trống " + d.get("trong") + ", giữ " + d.get("giu") + ", bán " + d.get("ban") + "; vé "
                + Chung.tien(Chung.so(t.get("gia_ve"))) + ", giữ " + Chung.g(Chung.sod(t.get("ttl_s"))) + " s");
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    /** Ghế trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat chụp SAU lần
     *  giữ của mình (tt["phien"] ≥ phien của kq). Giỏ vẫn giữ nguyên – lệnh "tt" sau đó sẽ bị máy chủ từ chối, đúng
     *  như khi trang thanh toán mở từ trước bấm "Thanh toán" quá hạn. */
    void capNhat(Map<String, Object> moi) {
        List<String> gio = new ArrayList<>();
        synchronized (khoa) {
            tt = moi;
            for (String g : gioHang) if (!daBao.contains(g)) gio.add(g);
            if (Chung.so(moi.get("phien")) < phienGio) return;
        }
        Map<String, Object> chu = Chung.map(moi.get("chu"));
        for (String g : gio) {
            Object ai = chu.get(g);
            if (ten.equals(ai)) continue;
            if (ai == null) log("  Hết thời gian giữ chỗ: " + g + " đã được nhả");
            else log("  Ghế " + g + " đã bị " + ai + " lấy mất!");
            synchronized (khoa) { daBao.add(g); }
        }
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    static List<String> chuoi(Object ds) {
        List<String> p = new ArrayList<>();
        for (Object o : Chung.list(ds)) p.add(String.valueOf(o));
        return p;
    }

    void giu(List<String> ghe) throws IOException {
        log("> giữ " + String.join(", ", ghe));
        kn.guiYc("giu_cho", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) {
                    gioHang = chuoi(tin.get("ghe"));
                    phienGio = tin.containsKey("phien") ? Chung.so(tin.get("phien")) : 0;
                    daBao = new HashSet<>();
                }
                log(Chung.f("  ĐÃ GIỮ (%.1f ms), thanh toán trong %.0f s", rtt, Chung.sod(tin.get("het_han_ms")) / 1000));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("ghe", new ArrayList<Object>(ghe)));
    }

    void thanhToan(List<String> ghe) throws IOException {
        log("> thanh toán " + String.join(", ", ghe));
        kn.guiYc("thanh_toan", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) { gioHang.removeAll(chuoi(tin.get("ghe"))); }
                log(Chung.f("  ĐẶT VÉ THÀNH CÔNG (%.1f ms): ", rtt) + Chung.tien(Chung.so(tin.get("tien"))));
                log("  mã vé " + String.join(", ", chuoi(tin.get("ma_ve"))));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("ghe", new ArrayList<Object>(ghe)));
    }

    void nha(List<String> ghe) throws IOException {
        log("> nhả " + String.join(", ", ghe));
        kn.guiYc("nha", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) { gioHang.removeAll(chuoi(tin.get("ghe"))); }
                log("  Đã nhả " + String.join(", ", chuoi(tin.get("ghe"))));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("ghe", new ArrayList<Object>(ghe)));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            List<String> gio;
            synchronized (khoa) { gio = new ArrayList<>(gioHang); }
            List<String> sau = new ArrayList<>();
            for (int i = 1; i < p.length; i++) sau.add(p[i].toUpperCase());
            if (p[0].equals("giu") && p.length >= 2) giu(sau);
            else if (p[0].equals("tt") || p[0].equals("nha")) {
                List<String> ghe = sau.isEmpty() ? gio : sau;
                if (ghe.isEmpty()) log("chưa giữ ghế nào");
                else if (p[0].equals("tt")) thanhToan(ghe);
                else nha(ghe);
            } else if (p[0].equals("xem")) {
                Map<String, Object> t;
                synchronized (khoa) { t = tt; }
                for (String d : soDoChu(t, ten)) log(d);
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: giu E7 [E8 …] | tt [ghế…] | nha [ghế…] | xem | thoat");
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43720;
        double t0 = Chung.bayGio();
        DatVe ng = new DatVe(vt.get(0));
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
