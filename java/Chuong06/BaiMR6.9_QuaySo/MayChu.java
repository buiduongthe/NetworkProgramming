import java.time.LocalTime;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.9 – Lõi máy chủ quay số trúng thưởng: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * Chương trình khuyến mãi "Vòng quay may mắn": 10 món quà, hàng chục nghìn lượt quay. Thao tác ghi duy nhất:
 *     {"loai":"quay","ma_yc":k[,"nguoi":"u17"]}
 * Luật: mỗi người quay ĐÚNG MỘT lần (quay lần nữa – dù bằng ma_yc khác, từ thiết bị khác – bị từ chối "da_quay" kèm
 * kết quả cũ: idempotency theo tên); mỗi lượt trúng với xác suất p (mặc định 1/2000) nếu còn quà; trúng thì nhận
 * ngẫu nhiên một món còn lại.
 *
 * Tài nguyên trong KhoTranhChap:
 *   - ô "qua"      : {"con": (mã các món còn lại)} – ô "nóng" mà mọi lượt TRÚNG cùng tranh;
 *   - ô "ng:<tên>" : null (chưa quay) | {"qua": mã|null} – một ô cho mỗi người, tạo khi người đó quay lần đầu.
 *                    Lượt không trúng chỉ chạm ô của chính mình nên các lượt chạy song song được (khóa theo tài
 *                    nguyên); chỉ lượt trúng mới khóa thêm ô "qua".
 * Rút thăm (nextDouble() < p) làm TRƯỚC, ngoài vùng găng: kết quả rút thăm không phụ thuộc trạng thái.
 *
 * Sổ kiểm toán đếm hai sai phạm:
 *   thua_qua    – một món quà đã trao lại được trao lần nữa (tổng số quà trao > số quà có);
 *   quay_2_lan  – một người có hai lượt quay được chấp nhận (hai thiết bị cùng bấm, ô của người đó bị ghi đè).
 */
final class MayChu {
    private MayChu() {}

    static final List<String> TEN_QUA = List.of("Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh",
            "Ba lô", "Nồi cơm điện", "Loa", "Phiếu 500k");

    /** 0.0005 -> "1/2000"; 0.3 -> "0.3". */
    static String chuP(double p) {
        if (p > 0) {
            long r = Math.round(1 / p);
            if (Math.abs(1 / p - r) < 1e-6 && r > 1) return "1/" + r;
        }
        return Chung.g(p);
    }

    static String gio() { return LocalTime.now().format(GiaoDienChung.GIO); }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("thua_qua", "thừa quà");
        m.put("quay_2_lan", "quay 2 lần");
        return m;
    }

    static final class SoKiemToanQuaySo extends Chung.SoKiemToan {
        final long soQua;
        final Set<Object> daQuay = new HashSet<>();          // người đã có lượt quay được chấp nhận (theo sổ)
        final Map<Long, Object> daTrao = new HashMap<>();    // mã quà -> người nhận đầu tiên
        long soTrao;                                         // số lượt trúng được chấp nhận

        SoKiemToanQuaySo(long soQua) {
            super(tenSaiPham());
            this.soQua = soQua;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            Object ng = gd.get("nguoi"), q = gd.get("qua");
            if (daQuay.contains(ng)) saiPham("quay_2_lan", ng + " quay lần 2");
            daQuay.add(ng);
            if (q != null) {
                soTrao++;
                long m = Chung.so(q);
                if (daTrao.containsKey(m)) saiPham("thua_qua", TEN_QUA.get((int) m) + ": " + daTrao.get(m) + " rồi " + ng);
                else daTrao.put(m, ng);
            }
        }

        synchronized long soLuot() { return giaoDich.size(); }

        synchronized long soTrung() { return soTrao; }
    }

    static Map<String, Object> thamSoMacDinh() { return Json.obj("so_qua", 10L, "p", 0.0005); }

    static final class MayChuQuaySo extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        Object khoaTao;                                      // tạo ô "ng:<tên>" đúng một lần
        Set<String> daTao;
        ArrayDeque<Map<String, Object>> nguoiTrung;          // mới nhất trước (để hiển thị), tối đa 50
        volatile boolean baoHet;

        MayChuQuaySo(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Quay số MR6.9", List.of("quay"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        SoKiemToanQuaySo so() { return (SoKiemToanQuaySo) so; }

        // ------------------------------------------------------------ tạo / đặt lại chương trình
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long soQua = Chung.so(ts.getOrDefault("so_qua", 1L));
            if (soQua < 1 || soQua > TEN_QUA.size()) throw new IllegalArgumentException("số quà 1–" + TEN_QUA.size());
            double p = Chung.sod(ts.getOrDefault("p", 0.0));
            if (!(p >= 0 && p <= 1)) throw new IllegalArgumentException("xác suất p trong [0, 1]");
        }

        @Override
        protected void khoiTao() {
            long soQua = tsL("so_qua");
            List<Long> con = new ArrayList<>();
            for (long i = 0; i < soQua; i++) con.add(i);
            kho.datO("qua", Json.obj("con", List.copyOf(con)));
            so = new SoKiemToanQuaySo(soQua);
            khoaTao = new Object();
            daTao = new HashSet<>();
            nguoiTrung = new ArrayDeque<>();
            baoHet = false;
        }

        private String oNguoi(String nguoi) {
            String k = "ng:" + nguoi;
            synchronized (khoaTao) {
                if (daTao.add(k)) kho.datO(k, null);
            }
            return k;
        }

        /** Đọc ô "qua" không khóa (để hiển thị, kiểm tra nhanh); đang đặt lại (ô chưa tạo) -> danh sách rỗng. */
        List<Object> conLai() {
            try {
                return Chung.list(kho.doc("qua").get("con"));
            } catch (NullPointerException e) {
                return List.of();
            }
        }

        // ------------------------------------------------------------ quay
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            String kNg = oNguoi(nguoi);
            boolean rutTrung = ThreadLocalRandom.current().nextDouble() < tsD("p");   // kim dừng ở ô quà với xác suất p
            List<String> cacKhoa = new ArrayList<>(List.of(kNg));
            if (rutTrung && !conLai().isEmpty()) cacKhoa.add("qua");   // hết quà thì khỏi chạm ô "qua" (quà không tăng lại)
            Chung.XuLy xuLy = doc -> {                                 // hàm THUẦN: đọc doc, trả về Kq(kq, ghi)
                Object cu = doc.get(kNg);
                if (cu != null) {
                    Object q = Chung.map(cu).get("qua");
                    String truoc = q == null ? "không trúng" : "trúng " + TEN_QUA.get((int) Chung.so(q));
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_quay", "ly_do", "đã quay rồi (" + truoc + ")",
                            "qua", q), null);
                }
                Object q = doc.get("qua");
                if (q != null && !Chung.list(Chung.map(q).get("con")).isEmpty()) {
                    List<Object> ds = Chung.list(Chung.map(q).get("con"));
                    Object chon = ds.get(ThreadLocalRandom.current().nextInt(ds.size()));
                    List<Object> con = new ArrayList<>();
                    for (Object x : ds) if (!x.equals(chon)) con.add(x);
                    return new Chung.Kq(Json.obj("ok", true, "trung", true, "qua", chon,
                            "ten_qua", TEN_QUA.get((int) Chung.so(chon)), "nguoi", nguoi, "con_lai", (long) con.size()),
                            Json.obj(kNg, Json.obj("qua", chon), "qua", Json.obj("con", List.copyOf(con))));
                }
                return new Chung.Kq(Json.obj("ok", true, "trung", false, "qua", null, "nguoi", nguoi),
                        Json.obj(kNg, Json.obj("qua", null)));
            };
            boolean gop = khach.soNguoi != null;                       // người ảo của bộ mô phỏng: không báo từng người
            kho.thucHien(cacKhoa, xuLy, traLoi, kq -> daQuay(kq, gop));
        }

        /** Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. */
        private void daQuay(Map<String, Object> kq, boolean gop) {
            so.ghi(Json.obj("nguoi", kq.get("nguoi"), "qua", kq.get("qua")));
            if (Chung.dung(kq.get("trung"))) {
                synchronized (nguoiTrung) {
                    nguoiTrung.addFirst(Json.obj("nguoi", kq.get("nguoi"), "qua", kq.get("qua"), "gio", gio()));
                    if (nguoiTrung.size() > 50) nguoiTrung.removeLast();
                }
                if (!gop) thongBao(kq.get("nguoi") + " trúng " + kq.get("ten_qua"));
                if (Chung.so(kq.get("con_lai")) == 0 && !baoHet) {
                    baoHet = true;
                    thongBao("Đã trao hết quà sau " + so().soLuot() + " lượt");
                }
            }
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            Set<Long> con = new HashSet<>();
            for (Object x : conLai()) con.add(Chung.so(x));
            List<Map<String, Object>> trung;
            synchronized (nguoiTrung) { trung = new ArrayList<>(nguoiTrung); }
            Map<Long, Object> nhan = new HashMap<>();
            for (int i = trung.size() - 1; i >= 0; i--) nhan.put(Chung.so(trung.get(i).get("qua")), trung.get(i).get("nguoi"));
            long soQua = tsL("so_qua");
            List<Object> qua = new ArrayList<>();
            for (long i = 0; i < soQua; i++)
                qua.add(Json.obj("ma", i, "ten", TEN_QUA.get((int) i), "con", con.contains(i),
                        "nguoi", con.contains(i) ? null : nhan.get(i)));
            return Json.obj("so_qua", soQua, "p", thamSo.get("p"), "con_lai", (long) con.size(), "qua", qua,
                    "da_quay", so().soLuot(), "so_trung", so().soTrung(),
                    "nguoi_trung", new ArrayList<Object>(trung.subList(0, Math.min(20, trung.size()))));
        }

        @Override
        protected String tomTat() {
            return "Trao " + so().soTrung() + "/" + tsL("so_qua") + " quà, " + so().soLuot() + " lượt, p " + chuP(tsD("p"));
        }
    }
}
