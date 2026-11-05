import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.11 – Người mua flash sale (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến (đếm ngược, mở bán, tồn kho, hết hàng) và nhận lệnh:
 *   mua [n]     đặt một đơn n sản phẩm (mặc định 1)          thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out FlashSale <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class FlashSale {
    final String ten;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    final Object khoa = new Object();
    final KetNoiKhach kn;

    FlashSale(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) { t = tt = kn.trangThai; }
        Map<String, Object> sp = Chung.map(t.get("sp"));
        log("Vào trang sale (" + t.get("che_do") + "): " + sp.get("ten"));
        log("  " + Chung.tien(Chung.so(sp.get("gia_goc"))) + " -> " + Chung.tien(Chung.so(sp.get("gia_sale"))) + ", kho "
                + t.get("ton") + "/" + t.get("ton_dau"));
        if (!Chung.dung(t.get("mo_ban"))) log(Chung.f("  mở bán sau %.1f s", Chung.sod(t.get("mo_con_ms")) / 1000));
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        Map<String, Object> cu;
        synchronized (khoa) {
            cu = tt;
            tt = moi;
        }
        if (Chung.so(moi.get("ton")) == 0 && Chung.so(cu.get("ton")) != 0) log("  kho: HẾT HÀNG");
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void mua(long sl) throws IOException {
        log("> MUA NGAY " + sl + " sản phẩm");
        kn.guiYc("mua", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok")))
                log("  ĐẶT HÀNG OK: đơn " + tin.get("ma_don") + ", " + Chung.tien(Chung.so(tin.get("thanh_tien")))
                        + Chung.f(" (%.1f ms)", rtt));
            else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("so_luong", sl));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p[0].equals("mua") && p.length <= 2) mua(p.length == 2 ? Long.parseLong(p[1]) : 1);
            else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: mua [n] | thoat");
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43750;
        double t0 = Chung.bayGio();
        FlashSale ng = new FlashSale(vt.get(0));
        ng.ketNoi(host, cong);
        CountDownLatch hetStdin = new CountDownLatch(1);
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, ng::lenh, hetStdin::countDown));
        if (thoiGian > 0) {
            while (ng.kn.mo && Chung.bayGio() - t0 < thoiGian) Thread.sleep(100);
        } else {
            while (ng.kn.mo && !hetStdin.await(100, TimeUnit.MILLISECONDS)) { }
            Thread.sleep(500);                                  // chờ trả lời của lệnh cuối
        }
        ng.kn.khiDong = () -> { };
        ng.kn.dong();
        System.exit(0);
    }
}
