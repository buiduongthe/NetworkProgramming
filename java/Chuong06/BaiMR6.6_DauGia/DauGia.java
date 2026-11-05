import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.6 – Người tham gia đấu giá (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến phiên (giá mới, bị trả giá cao hơn, gia hạn, người thắng) và nhận lệnh:
 *   tra <giá>       trả đúng giá này                tang [n]    trả giá hiện tại + n bước giá (mặc định 1)
 *   thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out DauGia <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class DauGia {
    final String ten;
    volatile Map<String, Object> tt;                 // cap_nhat mới nhất
    final Object khoa = new Object();
    final KetNoiKhach kn;

    DauGia(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t = tt = kn.trangThai;
        Map<String, Object> mon = Chung.map(t.get("mon"));
        String con = t.get("con_lai_ms") == null ? "không giới hạn" : Chung.f("còn %.1f s", Chung.sod(t.get("con_lai_ms")) / 1000);
        log("Vào phòng: " + mon.get("ten") + " (" + t.get("che_do") + ")");
        log("  giá " + Chung.tien(Chung.so(t.get("gia"))) + ", bước " + Chung.tien(Chung.so(mon.get("buoc_gia"))) + ", " + con);
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        Map<String, Object> cu;
        synchronized (khoa) { cu = tt; tt = moi; }
        if (Chung.so(moi.get("gia")) != Chung.so(cu.get("gia")) || !Objects.equals(moi.get("nguoi_dan_dau"), cu.get("nguoi_dan_dau"))) {
            log("Giá mới " + Chung.tien(Chung.so(moi.get("gia"))) + " – " + Chung.py(moi.get("nguoi_dan_dau")) + " dẫn đầu");
            if (ten.equals(cu.get("nguoi_dan_dau")) && !ten.equals(moi.get("nguoi_dan_dau"))) log("  Bạn bị trả giá cao hơn!");
        }
        if (Chung.dung(moi.get("ket_thuc")) && !Chung.dung(cu.get("ket_thuc")))
            log(ten.equals(moi.get("nguoi_dan_dau")) ? "  BẠN THẮNG!" : "  Bạn không thắng phiên này");
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void tra(long gia, String ghiChu) throws IOException {
        log("> trả " + Chung.tien(gia) + ghiChu);
        kn.guiYc("tra_gia", (tin, rtt) -> {
            if (Chung.dung(tin.get("ok"))) {
                String them = Chung.dung(tin.get("gia_han")) ? ", phiên được GIA HẠN" : "";
                log(Chung.f("  CHẤP NHẬN (%.1f ms)", rtt) + them);
            } else log("  TỪ CHỐI: " + tin.get("ly_do"));
        }, Json.obj("gia", gia));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p[0].equals("tra") && p.length == 2) tra(Long.parseLong(p[1]), "");
            else if (p[0].equals("tang") && p.length <= 2) {
                long n = p.length == 2 ? Long.parseLong(p[1]) : 1;
                long gia;
                synchronized (khoa) { gia = Chung.so(tt.get("gia")) + n * Chung.so(Chung.map(tt.get("mon")).get("buoc_gia")); }
                tra(gia, " (+" + n + " bước)");
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: tra <giá> | tang [n] | thoat");
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43700;
        double t0 = Chung.bayGio();
        DauGia ng = new DauGia(vt.get(0));
        ng.ketNoi(host, cong);
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
