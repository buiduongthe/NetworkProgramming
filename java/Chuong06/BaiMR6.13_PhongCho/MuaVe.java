import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.13 – Người mua vé (bản dòng lệnh).
 *
 * Kết nối máy chủ, in thông tin sự kiện, vị trí trong phòng chờ (số người phía trước thay đổi theo cap_nhat), lúc tới
 * lượt và kết quả mua; nhận lệnh:
 *   xep_hang        vào phòng chờ (rồi tự "chờ dài" cho_luot tới khi tới lượt)
 *   mua <n>         mua n vé          mua2 <n>   bấm ĐÚP: gửi hai yêu cầu mua liền nhau        thoat
 * --tu-dong: vào hàng ngay (phòng chờ tắt thì mua ngay), tới lượt thì tự mua --so-ve vé.
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out MuaVe <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]
 */
public class MuaVe {
    final String ten;
    final boolean tuDong;
    final long soVe;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    Long so;                                         // số thứ tự trong hàng
    Long truocCu;
    final Object khoa = new Object();
    final KetNoiKhach kn;

    MuaVe(String ten, boolean tuDong, long soVe) {
        this.ten = ten;
        this.tuDong = tuDong;
        this.soVe = soVe;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) { t = tt = kn.trangThai; }
        Map<String, Object> sk = Chung.map(t.get("su_kien"));
        log("Trang bán vé: " + sk.get("ten") + " (" + t.get("che_do") + ")");
        log("  còn " + t.get("con_lai") + "/" + sk.get("tong_ve") + " vé, " + Chung.tien(Chung.so(sk.get("gia_ve"))) + "/vé");
        log("  phòng chờ " + (Chung.dung(t.get("phong_cho")) ? "BẬT, cấp lượt " + Chung.g(Chung.sod(t.get("toc_do"))) + " người/s" : "TẮT"));
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        long truoc;
        boolean doi;
        synchronized (khoa) {
            tt = moi;
            if (so == null || truocCu == null) return;
            truoc = Math.max(0, so - Chung.so(moi.get("dang_phuc_vu")) - 1);
            doi = truoc != truocCu;
            truocCu = truoc;
        }
        if (doi && truoc > 0)
            log("  (hàng chờ) còn " + truoc + " người phía trước, " + Chung.f("~%.1f s", (truoc + 1) / Chung.sod(moi.get("toc_do"))));
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void xepHang() throws IOException {
        log("> vào phòng chờ");
        kn.guiYc("xep_hang", (tin, rtt) -> {
            if (!Chung.dung(tin.get("ok"))) {
                log("  TỪ CHỐI: " + tin.get("ly_do"));
                return;
            }
            synchronized (khoa) {
                so = Chung.so(tin.get("so"));
                truocCu = Chung.so(tin.get("truoc"));
            }
            log("  số thứ tự #" + tin.get("so") + ", còn " + tin.get("truoc") + " người phía trước, "
                    + Chung.f("~%.1f s", Chung.sod(tin.get("cho_s"))));
            try { kn.guiYc("cho_luot", this::denLuot, null); } catch (IOException e) { log("lỗi: " + e.getMessage()); }
        }, null);
    }

    void denLuot(Map<String, Object> tin, double rtt) {
        synchronized (khoa) { truocCu = null; }
        if (!Chung.dung(tin.get("ok"))) {
            log("  " + tin.get("ly_do"));
            return;
        }
        log(Chung.f("  ĐẾN LƯỢT! (chờ %.1f s) còn %.1f s để mua", rtt / 1000, Chung.sod(tin.get("han_ms")) / 1000));
        if (tuDong) {
            try { mua(soVe, 1); } catch (IOException e) { log("lỗi: " + e.getMessage()); }
        }
    }

    void mua(long n, int lan) throws IOException {
        log("> mua " + n + " vé" + (lan > 1 ? " (bấm đúp)" : ""));
        for (int i = 0; i < lan; i++)                           // bấm đúp: hai yêu cầu, không chờ trả lời
            kn.guiYc("mua", (tin, rtt) -> {
                if (Chung.dung(tin.get("ok")))
                    log(Chung.f("  THÀNH CÔNG (%.1f ms): đơn #", rtt) + tin.get("ma_don") + ", " + tin.get("so_ve") + " vé, "
                            + Chung.tien(Chung.so(tin.get("thanh_tien"))));
                else log(Chung.f("  TỪ CHỐI (%.1f ms): ", rtt) + tin.get("ly_do"));
            }, Json.obj("so_ve", n));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p[0].equals("xep_hang")) xepHang();
            else if ((p[0].equals("mua") || p[0].equals("mua2")) && p.length == 2) mua(Long.parseLong(p[1]), p[0].equals("mua2") ? 2 : 1);
            else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: xep_hang | mua <n> | mua2 <n> | thoat");
        } catch (IllegalArgumentException | IOException e) {
            log("lỗi: " + e.getMessage());
        }
    }

    public static void main(String[] args) throws Exception {
        List<String> vt = new ArrayList<>();
        double thoiGian = 0;
        boolean tuDong = false;
        long soVe = 2;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--tu-dong" -> tuDong = true;
                case "--so-ve" -> soVe = Long.parseLong(args[++i]);
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 1 ? vt.get(1) : "127.0.0.1";
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43770;
        double t0 = Chung.bayGio();
        MuaVe ng = new MuaVe(vt.get(0), tuDong, soVe);
        ng.ketNoi(host, cong);
        if (tuDong) {
            if (Chung.dung(ng.tt.get("phong_cho"))) ng.xepHang();
            else ng.mua(soVe, 1);
        }
        CountDownLatch hetStdin = new CountDownLatch(1);
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, ng::lenh, hetStdin::countDown));
        if (thoiGian > 0) {                                     // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
            while (ng.kn.mo && Chung.bayGio() - t0 < thoiGian) Thread.sleep(100);
        } else {                                                // tới khi hết stdin / thoat
            while (ng.kn.mo && !hetStdin.await(100, TimeUnit.MILLISECONDS)) { }
            Thread.sleep(300);                                  // chờ trả lời của lệnh cuối
        }
        ng.kn.khiDong = () -> { };
        ng.kn.dong();
        System.exit(0);
    }
}
