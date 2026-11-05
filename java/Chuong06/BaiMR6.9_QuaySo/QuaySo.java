import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.9 – Người tham gia vòng quay may mắn (bản dòng lệnh).
 *
 * Kết nối máy chủ, in diễn biến (ai trúng gì, số quà còn lại) và nhận lệnh:
 *   quay        quay một lượt
 *   quay x2     "bấm đúp": gửi HAI yêu cầu quay liên tiếp không chờ (hai ma_yc khác nhau) – máy chủ phải chấp nhận
 *               đúng một (idempotency theo tên người)
 *   gui_lai [k] gửi lại ĐÚNG ma_yc k (mặc định: của lượt quay gần nhất), như khi mạng chập chờn client gửi lại – máy
 *               chủ không quay lại mà trả lại kết quả cũ kèm "lap_lai": true (idempotency theo ma_yc)
 *   thoat
 * Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
 *
 * Chạy:  java -cp out QuaySo <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
 */
public class QuaySo {
    static String moTaKq(Map<String, Object> tin) {
        if (Chung.dung(tin.get("ok")))
            return Chung.dung(tin.get("trung")) ? "TRÚNG " + tin.get("ten_qua") + "!" : "Chúc may mắn lần sau";
        return "TỪ CHỐI: " + tin.get("ly_do");
    }

    final String ten;
    Map<String, Object> tt;                          // cap_nhat mới nhất
    Long maCuoi;                                     // ma_yc của lượt quay gần nhất (cho gui_lai)
    final Object khoa = new Object();
    final KetNoiKhach kn;

    QuaySo(String ten) {
        this.ten = ten;
        kn = new KetNoiKhach(this::capNhat, this::tinKhac, this::matKetNoi);
    }

    void log(String text) { Chung.inDong(ten, text); }

    void ketNoi(String host, int cong) throws IOException {
        kn.ketNoi(host, cong, ten);
        Map<String, Object> t;
        synchronized (khoa) { t = tt = kn.trangThai; }
        log("Vào vòng quay (" + t.get("che_do") + "): còn " + t.get("con_lai") + "/" + t.get("so_qua") + " quà, p "
                + MayChu.chuP(Chung.sod(t.get("p"))));
        kn.batDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void capNhat(Map<String, Object> moi) {
        Map<String, Object> cu;
        synchronized (khoa) {
            cu = tt;
            tt = moi;
        }
        if (Chung.so(moi.get("con_lai")) != Chung.so(cu.get("con_lai")))
            log("  còn " + moi.get("con_lai") + "/" + moi.get("so_qua") + " quà");
    }

    void tinKhac(Map<String, Object> tin) {
        if ("su_kien".equals(tin.get("loai"))) log("* " + tin.get("noi_dung"));
        else if ("kq".equals(tin.get("loai")) && Chung.dung(tin.get("lap_lai")))
            log("  (ma_yc " + tin.get("ma_yc") + ", lap_lai) " + moTaKq(tin));
    }

    void matKetNoi() { log("Mất kết nối máy chủ"); }

    // ------------------------------------------------------------ gửi
    void quay() throws IOException {
        long ma = kn.guiYc("quay", (tin, rtt) -> log("  ma_yc " + tin.get("ma_yc") + ": " + moTaKq(tin)
                + Chung.f(" (%.1f ms)", rtt)), null);
        synchronized (khoa) { maCuoi = ma; }
    }

    /** Gửi lại một ma_yc cũ: trường "ma_yc" truyền vào đè lên số mà KetNoiKhach tự đánh, nên dòng gửi đi mang
     *  đúng ma_yc k (như gửi thô). Trả lời "lap_lai" không khớp yêu cầu đang chờ nào -> đi vào tinKhac. */
    void guiLai(Long ma) throws IOException {
        synchronized (khoa) { if (ma == null) ma = maCuoi; }
        if (ma == null) {
            log("chưa quay lượt nào");
            return;
        }
        log("> gửi lại ma_yc " + ma);
        kn.guiYc("quay", null, Json.obj("ma_yc", ma));
    }

    void lenh(String dong) {
        String[] p = dong.split("\\s+");
        try {
            if (p.length == 1 && p[0].equals("quay")) {
                log("> quay");
                quay();
            } else if (p.length == 2 && p[0].equals("quay") && p[1].equals("x2")) {
                log("> quay x2 (bấm đúp: 2 yêu cầu liền nhau)");
                quay();
                quay();
            } else if (p[0].equals("gui_lai") && p.length <= 2) {
                guiLai(p.length == 2 ? Long.parseLong(p[1]) : null);
            } else if (p[0].equals("thoat")) {
                kn.khiDong = () -> { };
                kn.dong();
            } else log("lệnh: quay | quay x2 | gui_lai [k] | thoat");
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
        int cong = vt.size() > 2 ? Integer.parseInt(vt.get(2)) : 43730;
        double t0 = Chung.bayGio();
        QuaySo ng = new QuaySo(vt.get(0));
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
