import java.io.IOException;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.14 – Người đặt vé xe khách (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến (chỗ mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
 *   giu SG DL D01 [D02 …]   giữ chỗ đoạn Sài Gòn → Đà Lạt (mã trạm SG, BL, DL, NT; có thời hạn)
 *   tt                      thanh toán các chỗ vừa giữ          nha       nhả các chỗ vừa giữ
 *   xem                     in sơ đồ xe (mỗi chỗ 3 ký tự, một ký tự mỗi chặng)            thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out DatVeXe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class DatVeXe {
    /** Các dòng sơ đồ xe. Mỗi chỗ: mã rồi một ký tự mỗi chặng (. trống, g người khác giữ, b đã bán, G mình giữ, V vé
     *  của mình). */
    static List<String> soDoChu(Map<String, Object> tt, String ten) {
        List<Object> ghe = Chung.list(tt.get("ghe")), soDo = Chung.list(tt.get("so_do")), boTri = Chung.list(tt.get("bo_tri"));
        Map<String, Object> chu = Chung.map(tt.get("chu"));
        Map<String, String> o = new LinkedHashMap<>();
        int rong = 0, soTang = 1, soCot = 1;
        for (int i = 0; i < ghe.size(); i++) {
            String g = String.valueOf(ghe.get(i)), dong = String.valueOf(soDo.get(i));
            StringBuilder b = new StringBuilder(g + ":");
            for (int k = 0; k < dong.length(); k++) {
                char x = dong.charAt(k);
                boolean cuaToi = ten.equals(chu.get(g + "#" + k));
                b.append(cuaToi ? (x == 'g' ? 'G' : x == 'b' ? 'V' : x) : x);
            }
            o.put(g, b.toString());
            rong = Math.max(rong, b.length());
            List<Object> bt = Chung.list(boTri.get(i));
            soTang = Math.max(soTang, (int) Chung.so(bt.get(0)) + 1);
            soCot = Math.max(soCot, (int) Chung.so(bt.get(2)) + 1);
        }
        List<String> chang = new ArrayList<>();
        for (int k = 0; k < MayChu.SO_CHANG; k++) chang.add(MayChu.MA_TRAM.get(k) + "-" + MayChu.MA_TRAM.get(k + 1));
        List<String> ds = new ArrayList<>();
        ds.add("  " + tt.get("ten_xe") + " – chặng: " + String.join(" | ", chang));
        for (int tang = 0; tang < soTang; tang++) {
            if (soTang > 1) ds.add(tang == 0 ? "  Tầng dưới:" : "  Tầng trên:");
            TreeMap<Integer, Map<Integer, String>> hang = new TreeMap<>();
            for (int i = 0; i < ghe.size(); i++) {
                List<Object> bt = Chung.list(boTri.get(i));
                if ((int) Chung.so(bt.get(0)) != tang) continue;
                hang.computeIfAbsent((int) Chung.so(bt.get(1)), x -> new TreeMap<>()).put((int) Chung.so(bt.get(2)),
                        o.get(String.valueOf(ghe.get(i))));
            }
            for (Map<Integer, String> h : hang.values()) {
                StringBuilder b = new StringBuilder("   ");
                for (int c = 0; c < soCot; c++) {
                    String x = h.getOrDefault(c, "");
                    b.append(x).append(" ".repeat(rong - x.length())).append(c + 1 < soCot ? " " : "");
                }
                ds.add(b.toString().stripTrailing());
            }
        }
        return ds;
    }

    record Gio(List<String> ghe, int tu, int den) {}

    final String ten;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    Gio gioHang;                                     // chỗ vừa giữ được (để "tt"/"nha" không cần gõ lại)
    long phienGio;                                   // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    Set<String> daBao = new HashSet<>();             // ô trong giỏ đã báo hết hạn / bị lấy
    final Object khoa = new Object();
    final KetNoiKhach kn;

    DatVeXe(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) { t = tt = kn.trangThai; }
        Map<String, Object> d = Chung.map(t.get("dem"));
        log("Vào " + t.get("nha_xe") + ": chuyến " + t.get("chuyen") + ", " + String.valueOf(t.get("ten_xe")).toLowerCase()
                + " (" + t.get("che_do") + ")");
        List<String> tram = new ArrayList<>();
        for (Object x : Chung.list(t.get("tram"))) tram.add(String.valueOf(x));
        log("  tuyến " + String.join(" – ", tram) + "; chỗ-chặng trống " + d.get("trong") + ", giữ " + d.get("giu") + ", bán "
                + d.get("ban") + "; giữ chỗ " + Chung.g(Chung.sod(t.get("ttl_s"))) + " s");
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    /** Ô trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat chụp SAU lần giữ
     *  của mình (tt["phien"] ≥ phien của kq). */
    void capNhat(Map<String, Object> moi) {
        List<String> o = new ArrayList<>();
        synchronized (khoa) {
            tt = moi;
            if (gioHang == null || Chung.so(moi.get("phien")) < phienGio) return;
            for (String x : MayChu.oCua(gioHang.ghe(), gioHang.tu(), gioHang.den())) if (!daBao.contains(x)) o.add(x);
        }
        Map<String, Object> chu = Chung.map(moi.get("chu"));
        Map<String, Object> bao = new LinkedHashMap<>();
        for (String x : o) {
            Object ai = chu.get(x);
            if (ten.equals(ai)) continue;
            String g = x.split("#")[0];
            if (!bao.containsKey(g)) bao.put(g, ai);
            synchronized (khoa) { daBao.add(x); }
        }
        bao.forEach((g, ai) -> log(ai == null ? "  Hết thời gian giữ chỗ: " + g + " đã được nhả" : "  Chỗ " + g + " đã bị " + ai + " lấy mất!"));
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

    private Map<String, Object> truong(Gio g) {
        return Json.obj("ghe", new ArrayList<Object>(g.ghe()), "tu", (long) g.tu(), "den", (long) g.den());
    }

    void giu(Gio g) throws IOException {
        log("> giữ " + String.join(", ", g.ghe()) + " " + MayChu.tenDoan(g.tu(), g.den()));
        kn.guiYc("giu_cho", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) {
                    gioHang = new Gio(chuoi(tin.get("ghe")), g.tu(), g.den());
                    phienGio = tin.containsKey("phien") ? Chung.so(tin.get("phien")) : 0;
                    daBao = new HashSet<>();
                }
                log(Chung.f("  ĐÃ GIỮ (%.1f ms), ", rtt) + Chung.tien(Chung.so(tin.get("tien")))
                        + Chung.f(", thanh toán trong %.0f s", Chung.sod(tin.get("het_han_ms")) / 1000));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, truong(g));
    }

    void thanhToan(Gio g) throws IOException {
        log("> thanh toán " + String.join(", ", g.ghe()) + " " + MayChu.tenDoan(g.tu(), g.den()));
        kn.guiYc("thanh_toan", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) { gioHang = null; }
                log(Chung.f("  ĐẶT VÉ THÀNH CÔNG (%.1f ms): ", rtt) + Chung.tien(Chung.so(tin.get("tien"))));
                log("  mã vé " + String.join(", ", chuoi(tin.get("ma_ve"))));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, truong(g));
    }

    void nha(Gio g) throws IOException {
        log("> nhả " + String.join(", ", g.ghe()) + " " + MayChu.tenDoan(g.tu(), g.den()));
        kn.guiYc("nha", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                synchronized (khoa) { gioHang = null; }
                log("  Đã nhả " + String.join(", ", chuoi(tin.get("ghe"))));
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, truong(g));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            Gio gio;
            synchronized (khoa) { gio = gioHang; }
            if (p[0].equals("giu") && p.length >= 4 && MayChu.MA_TRAM.contains(p[1].toUpperCase())
                    && MayChu.MA_TRAM.contains(p[2].toUpperCase())) {
                List<String> ghe = new ArrayList<>();
                for (int i = 3; i < p.length; i++) ghe.add(p[i].toUpperCase());
                giu(new Gio(ghe, MayChu.MA_TRAM.indexOf(p[1].toUpperCase()), MayChu.MA_TRAM.indexOf(p[2].toUpperCase())));
            } else if (p[0].equals("tt") || p[0].equals("nha")) {
                if (gio == null) log("chưa giữ chỗ nào");
                else if (p[0].equals("tt")) thanhToan(gio);
                else nha(gio);
            } else if (p[0].equals("xem")) {
                Map<String, Object> t;
                synchronized (khoa) { t = tt; }
                for (String d : soDoChu(t, ten)) log(d);
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: giu SG DL D01 [D02 …] | tt | nha | xem | thoat   (trạm: " + String.join(", ", MayChu.MA_TRAM) + ")");
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43780;
        double t0 = Chung.bayGio();
        DatVeXe ng = new DatVeXe(vt.get(0));
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
