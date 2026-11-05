import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.6 – Lõi máy chủ đấu giá: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * Một món hàng, n người cùng trả giá. Thao tác ghi duy nhất:
 *     {"loai":"tra_gia","ma_yc":k,"gia":5300000[,"nguoi":"u17"]}
 * Luật: giá ≥ giá hiện tại + bước giá; phiên còn hạn. Trả giá khi phiên còn < gia_han_s giây (mặc định 10 s) thì
 * phiên được gia hạn tới "bây giờ + gia_han_s" ("đóng mềm" – soft close, chống "bắn tỉa" phút cuối). Hết giờ: máy
 * chủ đóng phiên, công bố người thắng.
 *
 * Trạng thái là MỘT ô "mon" của KhoTranhChap: {"gia","nguoi","so_lan","het_han","ket_thuc"}; mọi thay đổi tạo Map
 * mới (xuLy thuần) nên cả 4 chế độ dùng chung đúng một hàm nghiệp vụ.
 *
 * Sổ kiểm toán đếm hai sai phạm:
 *   gia_thap_hon – một giá được chấp nhận mà thấp hơn (giá cao nhất đã chấp nhận trước + bước giá): hai người
 *                  cùng đọc giá cũ, cùng "thắng" kiểm tra, người ghi sau đè người ghi trước;
 *   gia_cuoi_sai – giá/người dẫn đầu cuối cùng khác giá cao nhất đã chấp nhận (giá cao bị ghi đè mất).
 */
final class MayChu {
    private MayChu() {}

    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    static String gio() { return LocalTime.now().format(GIO); }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("gia_thap_hon", "giá thấp hơn");
        m.put("gia_cuoi_sai", "giá cuối sai");
        return m;
    }

    static final class SoKiemToanDauGia extends Chung.SoKiemToan {
        final long buoc;
        Long caoNhat;                    // giá cao nhất đã chấp nhận (theo sổ)
        String nguoiCaoNhat;

        SoKiemToanDauGia(long buocGia) {
            super(tenSaiPham());
            buoc = buocGia;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            long gia = Chung.so(gd.get("gia"));
            if (caoNhat != null && gia < caoNhat + buoc)
                saiPham("gia_thap_hon", gd.get("nguoi") + " " + Chung.tien(gia) + " sau " + Chung.tien(caoNhat));
            if (caoNhat == null || gia > caoNhat) {
                caoNhat = gia;
                nguoiCaoNhat = (String) gd.get("nguoi");
            }
        }

        @Override
        protected Map<String, Long> kiemTraCuoi(Map<String, Object> doc) {
            Map<String, Object> m = Chung.map(doc.get("mon"));
            if (m == null || caoNhat == null) return Map.of();
            boolean sai = Chung.so(m.get("gia")) != caoNhat || !java.util.Objects.equals(m.get("nguoi"), nguoiCaoNhat);
            return sai ? Map.of("gia_cuoi_sai", 1L) : Map.of();
        }
    }

    static Map<String, Object> thamSoMacDinh() {
        return Json.obj("ten_mon", "Bình gốm hoa lam", "gia_khoi_diem", 5_000_000L, "buoc_gia", 100_000L,
                "thoi_gian_s", 0.0, "gia_han_s", 10.0);                       // thoi_gian_s = 0: không giới hạn
    }

    static final class MayChuDauGia extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        LinkedList<Map<String, Object>> lichSu;          // giá đã chấp nhận, mới nhất trước (để hiển thị), ≤ 50
        volatile boolean dangDong;
        double lucBaoGiaHan;

        MayChuDauGia(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Đấu giá MR6.6", List.of("tra_gia"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        // ------------------------------------------------------------ tạo / đặt lại phiên
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            if (Chung.sod(ts.getOrDefault("buoc_gia", 1L)) <= 0 || Chung.sod(ts.getOrDefault("gia_khoi_diem", 0L)) < 0)
                throw new IllegalArgumentException("bước giá > 0, giá khởi điểm ≥ 0");
            if (Chung.sod(ts.getOrDefault("thoi_gian_s", 0.0)) < 0 || Chung.sod(ts.getOrDefault("gia_han_s", 0.0)) < 0)
                throw new IllegalArgumentException("thời gian ≥ 0");
            if (ts.containsKey("ten_mon")) {
                String t = String.valueOf(ts.get("ten_mon"));
                int n = t.codePointCount(0, t.length());
                if (n < 1 || n > 40) throw new IllegalArgumentException("tên món 1–40 ký tự");
            }
        }

        @Override
        protected void khoiTao() {
            double tg = tsD("thoi_gian_s");
            Double hetHan = tg > 0 ? Chung.bayGio() + tg : null;
            kho.datO("mon", Json.obj("gia", tsL("gia_khoi_diem"), "nguoi", null, "so_lan", 0L, "het_han", hetHan,
                    "ket_thuc", false));
            so = new SoKiemToanDauGia(tsL("buoc_gia"));
            lichSu = new LinkedList<>();
            dangDong = false;
            lucBaoGiaHan = -1e9;
        }

        // ------------------------------------------------------------ trả giá
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            Object g = tin.get("gia");
            if (!Chung.laSoNguyen(g) || Chung.so(g) <= 0) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "giá phải là số nguyên dương"));
                return;
            }
            long gia = Chung.so(g), buoc = tsL("buoc_gia");
            double giaHanS = tsD("gia_han_s");
            Chung.XuLy xuLy = doc -> {                                     // hàm THUẦN: đọc doc, trả về (kq, ghi)
                Map<String, Object> m = Chung.map(doc.get("mon"));
                double bayGio = Chung.bayGio();
                Object hh = m.get("het_han");
                long giaHt = Chung.so(m.get("gia"));
                if (Chung.dung(m.get("ket_thuc")) || (hh != null && bayGio >= Chung.sod(hh)))
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_phien", "ly_do", "phiên đã kết thúc",
                            "gia", giaHt, "nguoi_dan_dau", m.get("nguoi")), null);
                if (gia < giaHt + buoc)
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "gia_thap", "ly_do", "giá phải ≥ " + Chung.tien(giaHt + buoc),
                            "gia", giaHt, "nguoi_dan_dau", m.get("nguoi")), null);
                Map<String, Object> moi = Chung.sua(m, "gia", gia, "nguoi", nguoi, "so_lan", Chung.so(m.get("so_lan")) + 1);
                boolean giaHan = hh != null && Chung.sod(hh) - bayGio < giaHanS;
                if (giaHan) moi.put("het_han", bayGio + giaHanS);
                return new Chung.Kq(Json.obj("ok", true, "gia", gia, "nguoi_dan_dau", nguoi, "gia_han", giaHan),
                        Json.obj("mon", moi));
            };
            kho.thucHien(List.of("mon"), xuLy, traLoi, this::daChapNhan);
        }

        /** Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. */
        private void daChapNhan(Map<String, Object> kq) {
            so.ghi(Json.obj("nguoi", kq.get("nguoi_dan_dau"), "gia", kq.get("gia")));
            synchronized (lichSu) {
                lichSu.addFirst(Json.obj("nguoi", kq.get("nguoi_dan_dau"), "gia", kq.get("gia"), "gio", gio(),
                        "gia_han", kq.get("gia_han")));
                if (lichSu.size() > 50) lichSu.removeLast();
            }
            if (Chung.dung(kq.get("gia_han")) && Chung.bayGio() - lucBaoGiaHan > 1.0) {
                lucBaoGiaHan = Chung.bayGio();
                thongBao("Gia hạn: " + kq.get("nguoi_dan_dau") + " trả giá phút cuối, còn " + Chung.g(tsD("gia_han_s")) + " s");
            }
            baoDoi();
        }

        // ------------------------------------------------------------ đồng hồ phiên (luồng nền, 100 ms)
        @Override
        protected void dinhKy(double bayGio) {
            Map<String, Object> m = kho.doc("mon");
            Object hh = m.get("het_han");
            if (Chung.dung(m.get("ket_thuc")) || hh == null || bayGio < Chung.sod(hh) || dangDong) return;
            dangDong = true;
            Chung.XuLy xuLy = doc -> {                                     // đóng phiên cũng là một thao tác ghi
                Map<String, Object> x = Chung.map(doc.get("mon"));
                Object h = x.get("het_han");
                if (Chung.dung(x.get("ket_thuc")) || h == null || Chung.bayGio() < Chung.sod(h))
                    return new Chung.Kq(Json.obj("ok", false), null);     // vừa được gia hạn hoặc đã đóng
                return new Chung.Kq(Json.obj("ok", true, "nguoi", x.get("nguoi"), "gia", x.get("gia")),
                        Json.obj("mon", Chung.sua(x, "ket_thuc", true)));
            };
            kho.thucHien(List.of("mon"), xuLy, kq -> dangDong = false, this::daDong);
        }

        private void daDong(Map<String, Object> kq) {
            if (kq.get("nguoi") != null)
                thongBao("Kết thúc phiên: " + kq.get("nguoi") + " thắng với " + Chung.tien(Chung.so(kq.get("gia"))));
            else thongBao("Kết thúc phiên: không có ai trả giá");
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            Map<String, Object> m = kho.doc("mon");
            Object hh = m.get("het_han");
            Object conLai = hh == null ? null : (Object) Math.max(0L, Math.round((Chung.sod(hh) - Chung.bayGio()) * 1000));
            List<Map<String, Object>> ls;
            synchronized (lichSu) { ls = new ArrayList<>(lichSu.subList(0, Math.min(12, lichSu.size()))); }
            return Json.obj("mon", Json.obj("ten", thamSo.get("ten_mon"), "gia_khoi_diem", thamSo.get("gia_khoi_diem"),
                            "buoc_gia", thamSo.get("buoc_gia"), "gia_han_s", thamSo.get("gia_han_s")),
                    "gia", m.get("gia"), "nguoi_dan_dau", m.get("nguoi"), "so_lan", m.get("so_lan"),
                    "con_lai_ms", conLai, "ket_thuc", m.get("ket_thuc"), "lich_su", ls);
        }

        @Override
        protected String tomTat() {
            Map<String, Object> m = kho.doc("mon");
            Object ai = m.get("nguoi") == null ? "–" : m.get("nguoi");
            return "Giá cuối " + Chung.tien(Chung.so(m.get("gia"))) + ", " + (Chung.dung(m.get("ket_thuc")) ? "thắng" : "dẫn đầu") + " " + ai;
        }
    }
}
