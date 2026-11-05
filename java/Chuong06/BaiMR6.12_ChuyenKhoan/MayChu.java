import java.time.LocalTime;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.12 – Lõi máy chủ ngân hàng: chuyển khoản + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * 5–20 tài khoản, mỗi tài khoản là MỘT ô của KhoTranhChap (giá trị = số dư, số nguyên đồng, kiểu Long). Thao tác ghi:
 *     {"loai":"chuyen","ma_yc":k,"tu":"TK01","den":"TK02","so_tien":300000[,"noi_dung":"..."][,"nguoi":"u17"]}
 * Luật: trừ tài khoản nguồn, cộng tài khoản đích trong CÙNG một thao tác (hai ô); số dư nguồn không đủ thì từ chối.
 * Nhiều người chuyển chéo cùng lúc (A→B và B→A) nên một thao tác luôn đụng HAI tài nguyên:
 *   khoa            khóa hai tài khoản theo THỨ TỰ MÃ (TK01 trước TK02) -> mọi luồng xin khóa cùng thứ tự, không thể
 *                   có vòng chờ -> không khóa chết;
 *   khoa_ngay_tho=1 khóa nguồn rồi mới khóa đích: A→B giữ A chờ B, B→A giữ B chờ A -> KHÓA CHẾT (deadlock). Khung
 *                   phát hiện bằng hết thời gian chờ khóa (tryLock 2 s): giao dịch "nạn nhân" bị hủy, nhả khóa, giao
 *                   dịch kia đi tiếp; máy chủ báo "khóa chết: …";
 *   lac_quan        đọc (số dư, phiên bản) của cả hai tài khoản, rồi so sánh-và-đổi CẢ HAI một cách nguyên tử.
 *
 * Sổ kiểm toán ghi mọi giao dịch được chấp nhận và tự tính số dư theo sổ; đếm ba sai phạm:
 *   so_du_am  "thấu chi"  – theo sổ, một giao dịch làm tài khoản nguồn âm (hai luồng cùng thấy "đủ tiền" rồi cùng trừ);
 *   so_du_sai "lệch TK"   – số tài khoản có số dư cuối khác số dư theo sổ (mất cập nhật);
 *   tong_lech "tổng lệch" – tổng tiền toàn hệ thống khác tổng ban đầu (tiền tự sinh ra / mất đi).
 */
final class MayChu {
    private MayChu() {}

    static final List<String> CHU_TK = List.of("An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Lan", "Minh", "Nam",
            "Oanh", "Phúc", "Quân", "Sơn", "Tâm", "Uyên", "Vy", "Xuân", "Yến", "Thảo");

    static String maTk(int i) { return Chung.f("TK%02d", i + 1); }

    /** +300.000 đ / -300.000 đ / 0 đ. */
    static String tienDau(long x) { return (x > 0 ? "+" : "") + Chung.tien(x); }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("tong_lech", "tổng lệch");
        m.put("so_du_sai", "lệch TK");
        m.put("so_du_am", "thấu chi");
        return m;
    }

    static final class SoKiemToanNganHang extends Chung.SoKiemToan {
        final Map<String, Long> soDu;                     // số dư theo sổ (cộng trừ đúng từng giao dịch đã chấp nhận)
        final long tongDau;

        SoKiemToanNganHang(Map<String, Long> soDuDau) {
            super(tenSaiPham());
            soDu = new LinkedHashMap<>(soDuDau);
            long t = 0;
            for (long v : soDuDau.values()) t += v;
            tongDau = t;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            String tu = (String) gd.get("tu"), den = (String) gd.get("den");
            long x = Chung.so(gd.get("so_tien"));
            soDu.merge(tu, -x, Long::sum);
            soDu.merge(den, x, Long::sum);
            if (soDu.get(tu) < 0)
                saiPham("so_du_am", gd.get("nguoi") + " " + tu + "→" + den + ": " + tu + " còn " + Chung.tien(soDu.get(tu)));
        }

        @Override
        protected Map<String, Long> kiemTraCuoi(Map<String, Object> doc) {
            for (String k : soDu.keySet()) if (!doc.containsKey(k)) return Map.of();
            long sai = 0, tong = 0;
            for (Map.Entry<String, Long> e : soDu.entrySet()) {
                long v = Chung.so(doc.get(e.getKey()));
                if (v != e.getValue()) sai++;
                tong += v;
            }
            return Map.of("so_du_sai", sai, "tong_lech", tong != tongDau ? 1L : 0L);
        }
    }

    static Map<String, Object> thamSoMacDinh() { return Json.obj("so_tk", 10L, "so_du_dau", 5_000_000L, "khoa_ngay_tho", 0L); }

    static final class MayChuNganHang extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        volatile Map<String, String> chu;                 // mã tài khoản -> chủ tài khoản
        volatile List<String> dsTk;
        volatile long tongDau;
        ArrayDeque<Map<String, Object>> lichSu;           // giao dịch đã chấp nhận, mới nhất trước (tối đa 50)
        AtomicLong maGd;                                  // mã giao dịch tăng mãi, kể cả qua các lần đặt lại
        volatile long soGd;
        Object khoaKc;
        volatile long khoaChet;
        long kcChuaBao;
        double lucBaoKc;

        MayChuNganHang(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Ngân hàng MR6.12", List.of("chuyen"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        // ------------------------------------------------------------ tạo / đặt lại
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long soTk = Chung.so(ts.getOrDefault("so_tk", 10L));
            if (soTk < 5 || soTk > CHU_TK.size()) throw new IllegalArgumentException("số tài khoản 5–" + CHU_TK.size());
            if (Chung.so(ts.getOrDefault("so_du_dau", 0L)) < 0) throw new IllegalArgumentException("số dư ban đầu ≥ 0");
            long n = Chung.so(ts.getOrDefault("khoa_ngay_tho", 0L));
            if (n != 0 && n != 1) throw new IllegalArgumentException("khoa_ngay_tho = 0 hoặc 1");
        }

        @Override
        protected void khoiTao() {
            int soTk = (int) tsL("so_tk");
            long dau = tsL("so_du_dau");
            List<String> ds = new ArrayList<>();
            Map<String, String> c = new LinkedHashMap<>();
            Map<String, Long> soDu = new LinkedHashMap<>();
            for (int i = 0; i < soTk; i++) {
                String m = maTk(i);
                ds.add(m);
                kho.datO(m, dau);
                c.put(m, CHU_TK.get(i));
                soDu.put(m, dau);
            }
            kho.khoaNgayTho = tsL("khoa_ngay_tho") != 0;
            so = new SoKiemToanNganHang(soDu);
            chu = c;
            dsTk = ds;
            tongDau = soTk * dau;
            lichSu = new ArrayDeque<>();
            if (maGd == null) maGd = new AtomicLong();
            soGd = 0;
            khoaKc = new Object();
            khoaChet = 0;
            kcChuaBao = 0;
            lucBaoKc = 0.0;
        }

        // ------------------------------------------------------------ chuyển khoản
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            Object tu0 = tin.get("tu"), den0 = tin.get("den"), x0 = tin.get("so_tien");
            Object noiDung0 = tin.getOrDefault("noi_dung", "");
            if (!Chung.laSoNguyen(x0) || Chung.so(x0) <= 0) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "số tiền phải là số nguyên dương"));
                return;
            }
            Map<String, String> c = chu;
            if (!c.containsKey(tu0) || !c.containsKey(den0)) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "khong_co", "ly_do", "tài khoản không tồn tại"));
                return;
            }
            if (tu0.equals(den0)) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "tài khoản nguồn và đích trùng nhau"));
                return;
            }
            if (!(noiDung0 instanceof String nd) || nd.codePointCount(0, nd.length()) > 40) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "nội dung tối đa 40 ký tự"));
                return;
            }
            String tu = (String) tu0, den = (String) den0, noiDung = (String) noiDung0;
            long x = Chung.so(x0);
            Chung.XuLy xuLy = doc -> {                                 // hàm THUẦN: đọc hai số dư, trả về Kq(kq, ghi)
                long a = Chung.so(doc.get(tu)), b = Chung.so(doc.get(den));
                if (a < x)
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_du", "ly_do",
                            "không đủ số dư (" + tu + " còn " + Chung.tien(a) + ")", "so_du", a), null);
                return new Chung.Kq(Json.obj("ok", true, "tu", tu, "den", den, "so_tien", x, "so_du", a - x, "nguoi", nguoi,
                        "noi_dung", noiDung), Json.obj(tu, a - x, den, b + x));
            };
            Consumer<Map<String, Object>> tra = kq -> {
                if ("khoa_cho_lau".equals(kq.get("ma_loi"))) kq = baoKhoaChet(nguoi, tu, den, kq);
                traLoi.accept(kq);
            };
            // thứ tự [nguồn, đích]: chế độ khoa sắp xếp lại theo mã, khóa ngây thơ thì giữ nguyên
            kho.thucHien(List.of(tu, den), xuLy, tra, this::daChuyen);
        }

        /** Gọi trong vùng găng ngay sau khi ghi hai số dư: ghi sổ kiểm toán đúng thứ tự ghi. */
        private void daChuyen(Map<String, Object> kq) {
            kq.put("ma_gd", maGd.incrementAndGet());
            synchronized (lichSu) {
                soGd++;
                lichSu.addFirst(Json.obj("ma", kq.get("ma_gd"), "gio", LocalTime.now().format(GiaoDienChung.GIO),
                        "nguoi", kq.get("nguoi"), "tu", kq.get("tu"), "den", kq.get("den"), "so_tien", kq.get("so_tien"),
                        "noi_dung", kq.get("noi_dung")));
                if (lichSu.size() > 50) lichSu.removeLast();
            }
            so.ghi(Json.obj("nguoi", kq.get("nguoi"), "tu", kq.get("tu"), "den", kq.get("den"), "so_tien", kq.get("so_tien")));
            baoDoi();
        }

        /** Chờ khóa quá 2 s: với khóa ngây thơ đó là khóa chết. Báo (tối đa 1 dòng/giây) và hủy giao dịch. */
        private Map<String, Object> baoKhoaChet(String nguoi, String tu, String den, Map<String, Object> kq) {
            String k = String.valueOf(kq.get("ly_do")).split(" ")[2];  // "chờ khóa TK02 quá 2 s (khóa chết?)"
            boolean ngayTho = kho.khoaNgayTho;
            boolean bao;
            long them = 0;
            synchronized (khoaKc) {
                khoaChet++;
                double bayGio = Chung.bayGio();
                bao = bayGio - lucBaoKc >= 1.0;
                if (bao) {
                    lucBaoKc = bayGio;
                    them = kcChuaBao;
                    kcChuaBao = 0;
                } else kcChuaBao++;
            }
            String s = Chung.g(Chung.CHO_KHOA_TOI_DA_S);
            if (bao)
                thongBao((ngayTho ? "khóa chết" : "chờ khóa quá hạn") + ": " + nguoi + " " + tu + "→" + den + ", chờ " + k
                        + " quá " + s + " s" + (them > 0 ? " (+" + them + " vụ)" : ""));
            String lyDo = (ngayTho ? "khóa chết: " : "") + "chờ " + k + " quá " + s + " s, đã hủy giao dịch";
            return Chung.sua(kq, "ly_do", lyDo);
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            Map<String, Object> snap = kho.anhChup();
            List<Object> tk = new ArrayList<>();
            long tong = 0;
            for (String m : dsTk) {
                long v = Chung.so(snap.get(m));
                tong += v;
                tk.add(Json.obj("ma", m, "chu", chu.get(m), "so_du", v));
            }
            List<Object> ls;
            synchronized (lichSu) { ls = new ArrayList<>(lichSu); }
            return Json.obj("tai_khoan", tk, "tong", tong, "tong_dau", tongDau, "khoa_ngay_tho", kho.khoaNgayTho,
                    "khoa_chet", khoaChet, "so_gd", soGd, "lich_su", new ArrayList<>(ls.subList(0, Math.min(20, ls.size()))));
        }

        @Override
        protected String tomTat() {
            Map<String, Object> snap = kho.anhChup();
            long tong = 0;
            for (String m : dsTk) tong += Chung.so(snap.get(m));
            long lech = tong - tongDau;
            if (lech != 0) return "Tổng tiền " + Chung.tien(tong) + ", LỆCH " + tienDau(lech);
            return "Tổng tiền " + Chung.tien(tong) + " (khớp), " + dsTk.size() + " tài khoản";
        }
    }
}
