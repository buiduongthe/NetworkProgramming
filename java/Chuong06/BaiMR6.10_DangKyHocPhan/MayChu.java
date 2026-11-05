import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.10 – Lõi máy chủ đăng ký học phần: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * Cổng đăng ký học phần ngày mở đăng ký: vài lớp học phần (mặc định 3 lớp, sĩ số 50), hàng nghìn sinh viên cùng bấm
 * "Đăng ký". Hai thao tác ghi:
 *     {"loai":"dang_ky","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
 *     {"loai":"huy","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
 * Luật: lớp còn chỗ (số đã đăng ký < sĩ số) mới nhận; sinh viên đã có trong lớp thì không đăng ký lại ("da_dang_ky");
 * chỉ hủy được lớp mình đã đăng ký ("chua_dang_ky").
 *
 * Mỗi lớp là MỘT ô của KhoTranhChap: {"sv": Set (bất biến) tên sinh viên}; ghi = tạo Set mới (xuLy thuần). Khóa theo
 * tài nguyên: hai lớp khác nhau được xử lý song song; một lớp "hot" là điểm nóng.
 *
 * Sổ kiểm toán (danh sách lớp theo sổ, ghi trong vùng găng) đếm hai sai phạm:
 *   vuot_si_so – một lượt đăng ký được chấp nhận khi lớp (theo sổ) đã đủ sĩ số;
 *   dk_trung   – một sinh viên được chấp nhận đăng ký một lớp mà sổ ghi là đã có tên (hai thiết bị cùng bấm, hoặc
 *                bấm đúp, cùng đọc "chưa có tên").
 */
final class MayChu {
    private MayChu() {}

    /** mã lớp, tên học phần, giảng viên, lịch học, số tín chỉ. */
    record Lop(String ma, String ten, String gv, String lich, long tc) {}

    static final List<Lop> DANH_MUC = List.of(
            new Lop("123013-01", "Lập trình mạng", "TS. Lê Hoàng Nam", "T2, tiết 1–3, E301", 3),
            new Lop("123005-01", "Quản trị mạng", "ThS. Phạm Thu Hà", "T3, tiết 4–6, E302", 3),
            new Lop("006124-01", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T4, tiết 7–9, E303", 2),
            new Lop("123013-02", "Lập trình mạng", "TS. Lê Hoàng Nam", "T5, tiết 1–3, E301", 3),
            new Lop("123005-02", "Quản trị mạng", "ThS. Phạm Thu Hà", "T6, tiết 4–6, E302", 3),
            new Lop("006124-02", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T7, tiết 7–9, E303", 2));

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("vuot_si_so", "vượt sĩ số");
        m.put("dk_trung", "ĐK trùng");
        return m;
    }

    static final class SoKiemToanDangKy extends Chung.SoKiemToan {
        final long siSo;
        final Map<String, Set<Object>> lop = new HashMap<>();          // mã lớp -> sinh viên theo sổ

        SoKiemToanDangKy(long siSo) {
            super(tenSaiPham());
            this.siSo = siSo;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            String ma = (String) gd.get("ma_lop");
            Object ng = gd.get("nguoi");
            Set<Object> ds = lop.computeIfAbsent(ma, x -> new HashSet<>());
            if ("huy".equals(gd.get("thao_tac"))) {
                ds.remove(ng);
                return;
            }
            if (ds.contains(ng)) saiPham("dk_trung", ng + " ĐK lại " + ma);
            else if (ds.size() >= siSo) saiPham("vuot_si_so", ma + ": " + ng + " là SV thứ " + (ds.size() + 1));
            ds.add(ng);
        }

        synchronized long tongSo() {
            long s = 0;
            for (Set<Object> x : lop.values()) s += x.size();
            return s;
        }

        synchronized long soLop(String ma) {
            Set<Object> x = lop.get(ma);
            return x == null ? 0 : x.size();
        }
    }

    static Map<String, Object> thamSoMacDinh() { return Json.obj("so_lop", 3L, "si_so", 50L); }

    static final class MayChuDangKy extends Chung.MayChuCoSo {
        volatile List<Lop> lop;                                       // do khoiTao() đặt

        MayChuDangKy(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Đăng ký học phần MR6.10", List.of("dang_ky", "huy"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        SoKiemToanDangKy so() { return (SoKiemToanDangKy) so; }

        // ------------------------------------------------------------ tạo / đặt lại đợt đăng ký
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long soLop = Chung.so(ts.getOrDefault("so_lop", 1L)), siSo = Chung.so(ts.getOrDefault("si_so", 1L));
            if (soLop < 1 || soLop > DANH_MUC.size()) throw new IllegalArgumentException("số lớp 1–" + DANH_MUC.size());
            if (siSo < 1 || siSo > 500) throw new IllegalArgumentException("sĩ số 1–500");
        }

        @Override
        protected void khoiTao() {
            lop = DANH_MUC.subList(0, (int) tsL("so_lop"));
            for (Lop x : lop) kho.datO("lop:" + x.ma(), Json.obj("sv", Set.of()));
            so = new SoKiemToanDangKy(tsL("si_so"));
        }

        @SuppressWarnings("unchecked")
        Set<String> svLop(String ma) {                                // đọc không khóa để hiển thị; đang đặt lại -> rỗng
            try {
                return (Set<String>) kho.doc("lop:" + ma).get("sv");
            } catch (NullPointerException e) {
                return Set.of();
            }
        }

        // ------------------------------------------------------------ đăng ký / hủy
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            Object ma = tin.get("ma_lop");
            boolean co = false;
            for (Lop x : lop) co |= x.ma().equals(ma);
            if (!co) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "không có lớp " + Chung.py(ma)));
                return;
            }
            String k = "lop:" + ma;
            long siSo = tsL("si_so");
            String thaoTac = (String) tin.get("loai");
            Chung.XuLy xuLy = doc -> {                                 // hàm THUẦN: đọc doc, trả về Kq(kq, ghi)
                @SuppressWarnings("unchecked")
                Set<String> sv = (Set<String>) Chung.map(doc.get(k)).get("sv");
                Set<String> moi = new HashSet<>(sv);
                if (thaoTac.equals("dang_ky")) {
                    if (sv.contains(nguoi))
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_dang_ky", "ly_do", "đã đăng ký lớp này",
                                "ma_lop", ma, "da_dk", (long) sv.size()), null);
                    if (sv.size() >= siSo)
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "lop_day", "ly_do", "lớp đã đủ", "ma_lop", ma,
                                "da_dk", (long) sv.size()), null);
                    moi.add(nguoi);
                } else {
                    if (!sv.contains(nguoi))
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "chua_dang_ky", "ly_do", "chưa đăng ký lớp này",
                                "ma_lop", ma, "da_dk", (long) sv.size()), null);
                    moi.remove(nguoi);
                }
                return new Chung.Kq(Json.obj("ok", true, "thao_tac", thaoTac, "ma_lop", ma, "nguoi", nguoi,
                        "da_dk", (long) moi.size()), Json.obj(k, Json.obj("sv", Collections.unmodifiableSet(moi))));
            };
            boolean gop = khach.soNguoi != null;
            kho.thucHien(List.of(k), xuLy, traLoi, kq -> daGhi(kq, gop));
        }

        /** Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. */
        private void daGhi(Map<String, Object> kq, boolean gop) {
            so.ghi(Json.obj("thao_tac", kq.get("thao_tac"), "nguoi", kq.get("nguoi"), "ma_lop", kq.get("ma_lop")));
            if (!gop) {                                                // người dùng thật: báo lên nhật ký
                String viec = "dang_ky".equals(kq.get("thao_tac")) ? "đăng ký" : "hủy";
                thongBao(kq.get("nguoi") + " " + viec + " " + kq.get("ma_lop") + " (" + kq.get("da_dk") + "/" + tsL("si_so") + ")");
            }
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            long siSo = tsL("si_so");
            List<Object> ds = new ArrayList<>();
            for (Lop x : lop) {
                List<String> sv = new ArrayList<>(svLop(x.ma()));
                Collections.sort(sv);
                ds.add(Json.obj("ma", x.ma(), "ten", x.ten(), "gv", x.gv(), "lich", x.lich(), "tc", x.tc(), "si_so", siSo,
                        "da_dk", (long) sv.size(), "theo_so", so().soLop(x.ma()), "sv", new ArrayList<Object>(sv)));
            }
            return Json.obj("si_so", siSo, "lop", ds);
        }

        @Override
        protected String tomTat() {
            long cho = tsL("so_lop") * tsL("si_so");
            return "Đã ĐK " + so().tongSo() + "/" + cho + " chỗ (" + tsL("so_lop") + " lớp × " + tsL("si_so") + ")";
        }
    }
}
