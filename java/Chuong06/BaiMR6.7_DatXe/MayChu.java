import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.7 – Lõi máy chủ đặt xe: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * m chiếc xe (mặc định 1), mỗi xe MỘT chỗ; n người cùng bấm "Đặt xe". Hai thao tác ghi:
 *     {"loai":"dat_xe","ma_yc":k[,"don":[x,y],"den":[x,y]][,"nguoi":"u17"]}
 *     {"loai":"huy","ma_yc":k[,"nguoi":"u17"]}
 * Luật: mỗi người chỉ giữ một chuyến; máy chủ giao xe TRỐNG gần điểm đón nhất; hết xe trống -> "hết chỗ". Hủy chuyến
 * trả xe về trạng thái trống.
 *
 * Bản đồ là lưới 5 × 5 đường (tọa độ 0–1, 1 đơn vị = 5 km); điểm đón/đến là ngã tư. Mỗi xe là MỘT ô "xe1", "xe2"…
 * của KhoTranhChap: {"ma","tai_xe","bien_so","mau","vi_tri","khach","don","den","gio","cuoc","phut"}. Vì phải biết
 * người đặt đã có chuyến chưa và xe nào trống gần nhất, đặt xe đọc–ghi CẢ m ô trong một thao tác (khoa: giữ khóa mọi
 * xe theo thứ tự tên ô; lac_quan: so sánh phiên bản mọi xe).
 *
 * Sổ kiểm toán đếm một sai phạm:
 *   trung_cho – một lần đặt được chấp nhận cho một xe mà (theo sổ) đang chở người khác: hai người cùng đọc "xe
 *               trống", cùng được báo "Đặt thành công – tài xế Hùng", người ghi sau đè người ghi trước, người kia
 *               đứng đường chờ một chiếc xe không đến.
 */
final class MayChu {
    private MayChu() {}

    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    static String gio() { return LocalTime.now().format(GIO); }

    static final double[] DUONG = {0.1, 0.3, 0.5, 0.7, 0.9};        // tọa độ 5 đường dọc / 5 đường ngang
    static final String[] TEN_DOC = {"Lê Lợi", "Hai Bà Trưng", "Nguyễn Huệ", "Pasteur", "Đồng Khởi"};
    static final String[] TEN_NGANG = {"Lý Tự Trọng", "Lê Thánh Tôn", "Nguyễn Du", "Võ Văn Tần", "Điện Biên Phủ"};
    static final List<double[]> NGA_TU = new ArrayList<>();
    static {
        for (double y : DUONG) for (double x : DUONG) NGA_TU.add(new double[] {x, y});
    }
    static final double KM_MOI_DON_VI = 5.0;
    static final String[][] TAI_XE = {{"Hùng", "51A-123.45", "trắng"}, {"Lan", "51G-678.90", "đỏ"},
            {"Minh", "51F-246.80", "xanh"}, {"Tuấn", "51H-135.79", "bạc"}, {"Hoa", "51K-864.20", "vàng"},
            {"Phúc", "51A-975.31", "đen"}, {"Quang", "51G-112.23", "cam"}, {"Thảo", "51F-445.56", "tím"},
            {"Sơn", "51H-778.89", "xám"}, {"Vy", "51K-990.01", "hồng"}};
    static final double[][] VI_TRI_XE = {{0.9, 0.1}, {0.1, 0.9}, {0.5, 0.3}, {0.3, 0.7}, {0.7, 0.5}, {0.1, 0.3},
            {0.9, 0.7}, {0.5, 0.9}, {0.3, 0.1}, {0.7, 0.9}};

    static int chiSo(double v) {
        for (int i = 0; i < DUONG.length; i++) if (Math.abs(DUONG[i] - v) < 1e-9) return i;
        return -1;
    }

    /** (0.1, 0.5) -> "Lê Lợi × Nguyễn Du". */
    static String tenDiem(double[] p) { return TEN_DOC[chiSo(p[0])] + " × " + TEN_NGANG[chiSo(p[1])]; }

    /** Đường đi trên lưới phố (Manhattan) giữa hai điểm, km. */
    static double km(double[] a, double[] b) { return (Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1])) * KM_MOI_DON_VI; }

    /** Giá cước: 12.000 đ mở cửa + 10.000 đ/km, làm tròn tới 1.000 đ. */
    static long cuoc(double[] a, double[] b) { return Math.round((12_000 + 10_000 * km(a, b)) / 1000) * 1000; }

    /** Thời gian xe tới điểm đón (30 km/h = 0,5 km/phút), tối thiểu 1 phút. */
    static long phutDen(double[] xe, double[] don) { return Math.max(1, Math.round(km(xe, don) / 0.5)); }

    static List<Object> dsDiem(double[] p) { return new ArrayList<>(List.of(p[0], p[1])); }

    static double[] diemCua(Object o) {
        List<Object> l = Chung.list(o);
        return new double[] {Chung.sod(l.get(0)), Chung.sod(l.get(1))};
    }

    /** Kiểm tra [x, y] là một ngã tư trên bản đồ; trả về tọa độ hoặc null. */
    static double[] diem(Object x) {
        if (!(x instanceof List<?> l) || l.size() != 2) return null;
        for (Object v : l) if (!(v instanceof Number)) return null;
        double[] p = {Math.round(Chung.sod(l.get(0)) * 100) / 100.0, Math.round(Chung.sod(l.get(1)) * 100) / 100.0};
        for (double[] q : NGA_TU) if (q[0] == p[0] && q[1] == p[1]) return q;
        return null;
    }

    static final class SoKiemToanDatXe extends Chung.SoKiemToan {
        final Map<Long, List<String>> trenXe = new LinkedHashMap<>();   // theo sổ: ai đang "có" chỗ trên xe i
        long soDat;                                                  // số lần đặt được chấp nhận

        SoKiemToanDatXe(long soXe) {
            super(new LinkedHashMap<>(Map.of("trung_cho", "trùng chỗ")));
            for (long i = 1; i <= soXe; i++) trenXe.put(i, new ArrayList<>());
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            List<String> ds = trenXe.get(Chung.so(gd.get("xe")));
            String nguoi = (String) gd.get("nguoi");
            if ("dat".equals(gd.get("loai"))) {
                soDat++;
                if (!ds.isEmpty()) saiPham("trung_cho", "xe " + gd.get("xe") + ": " + nguoi + " khi " + ds.get(ds.size() - 1) + " đang đặt");
                ds.add(nguoi);
            } else ds.remove(nguoi);                                 // hủy
        }
    }

    static final class MayChuDatXe extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        List<String> cacO;
        LinkedList<Map<String, Object>> lichSu;
        AtomicLong demGhi;                     // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
        volatile long phien;

        MayChuDatXe(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Đặt xe MR6.7", List.of("dat_xe", "huy"), Json.obj("so_xe", 1L), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        // ------------------------------------------------------------ tạo / đặt lại
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long m = Chung.so(ts.getOrDefault("so_xe", 1L));
            if (m < 1 || m > 50) throw new IllegalArgumentException("số xe 1–50");
        }

        @Override
        protected void khoiTao() {
            int m = (int) tsL("so_xe");
            List<String> o = new ArrayList<>();
            for (int i = 0; i < m; i++) {
                String[] tx = TAI_XE[i % TAI_XE.length];
                String ten = tx[0];
                if (i >= TAI_XE.length) ten = ten + " " + (i / TAI_XE.length + 1);
                double[] viTri = i < VI_TRI_XE.length ? VI_TRI_XE[i] : NGA_TU.get((i * 7) % NGA_TU.size());
                kho.datO("xe" + (i + 1), Json.obj("ma", (long) (i + 1), "tai_xe", ten, "bien_so", tx[1], "mau", tx[2],
                        "vi_tri", dsDiem(viTri), "khach", null, "don", null, "den", null, "gio", null, "cuoc", null, "phut", null));
                o.add("xe" + (i + 1));
            }
            cacO = o;
            so = new SoKiemToanDatXe(m);
            lichSu = new LinkedList<>();
            if (demGhi == null) {
                demGhi = new AtomicLong();
                phien = 0;
            }
        }

        // ------------------------------------------------------------ đặt xe / hủy
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            boolean that = khach.soNguoi == null;              // người dùng thật (không phải bộ mô phỏng): ghi nhật ký
            if ("dat_xe".equals(tin.get("loai"))) {
                double[] don = tin.get("don") == null ? new double[] {0.5, 0.5} : diem(tin.get("don"));
                double[] den = tin.get("den") == null ? new double[] {0.9, 0.9} : diem(tin.get("den"));
                if (don == null || den == null) {
                    traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "điểm đón/đến phải là ngã tư [x, y]"));
                    return;
                }
                Chung.XuLy xuLy = doc -> {                          // hàm THUẦN: đọc mọi xe, chọn xe trống gần nhất
                    for (Object v : doc.values()) {
                        Map<String, Object> xe = Chung.map(v);
                        if (nguoi.equals(xe.get("khach")))
                            return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_dat", "ly_do", "bạn đã đặt xe " + xe.get("tai_xe")), null);
                    }
                    Map<String, Object> chon = null;
                    double tot = 0;
                    for (Object v : doc.values()) {
                        Map<String, Object> xe = Chung.map(v);
                        if (xe.get("khach") != null) continue;
                        double d = km(diemCua(xe.get("vi_tri")), don);
                        if (chon == null || d < tot || (d == tot && Chung.so(xe.get("ma")) < Chung.so(chon.get("ma")))) {
                            chon = xe;
                            tot = d;
                        }
                    }
                    if (chon == null)
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_cho", "ly_do", "hết chỗ – không còn xe trống"), null);
                    long phut = phutDen(diemCua(chon.get("vi_tri")), don), tien = cuoc(don, den);
                    Map<String, Object> moi = Chung.sua(chon, "khach", nguoi, "don", dsDiem(don), "den", dsDiem(den), "gio", gio(),
                            "cuoc", tien, "phut", phut);
                    Map<String, Object> kq = Json.obj("ok", true, "xe", chon.get("ma"), "khach", nguoi, "tai_xe", chon.get("tai_xe"),
                            "bien_so", chon.get("bien_so"), "mau", chon.get("mau"), "phut", phut, "cuoc", tien);
                    return new Chung.Kq(kq, Json.obj("xe" + chon.get("ma"), moi));
                };
                kho.thucHien(cacO, xuLy, traLoi, kq -> daGhi("dat", kq, that));
            } else {
                Chung.XuLy xuLy = doc -> {
                    for (Object v : doc.values()) {
                        Map<String, Object> xe = Chung.map(v);
                        if (nguoi.equals(xe.get("khach"))) {
                            Map<String, Object> moi = Chung.sua(xe, "khach", null, "don", null, "den", null, "gio", null,
                                    "cuoc", null, "phut", null);
                            return new Chung.Kq(Json.obj("ok", true, "xe", xe.get("ma"), "khach", nguoi, "tai_xe", xe.get("tai_xe")),
                                    Json.obj("xe" + xe.get("ma"), moi));
                        }
                    }
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "chua_dat", "ly_do", "bạn chưa đặt xe"), null);
                };
                kho.thucHien(cacO, xuLy, traLoi, kq -> daGhi("huy", kq, that));
            }
        }

        /** Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự lần ghi;
         *  cap_nhat mang "phien" lúc chụp -> client biết một cap_nhat chụp trước hay sau lần ghi của mình. */
        private void daGhi(String loai, Map<String, Object> kq, boolean that) {
            long p = demGhi.incrementAndGet();
            phien = p;
            kq.put("phien", p);
            so.ghi(Json.obj("loai", loai, "xe", kq.get("xe"), "nguoi", kq.get("khach")));
            synchronized (lichSu) {
                lichSu.addFirst(Json.obj("gio", gio(), "loai", loai, "xe", kq.get("xe"), "tai_xe", kq.get("tai_xe"),
                        "nguoi", kq.get("khach")));
                if (lichSu.size() > 30) lichSu.removeLast();
            }
            if (that) {
                if (loai.equals("dat")) thongBao("Xe " + kq.get("xe") + " (" + kq.get("tai_xe") + ") nhận khách " + kq.get("khach"));
                else thongBao(kq.get("khach") + " hủy chuyến, xe " + kq.get("xe") + " trống");
            }
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            long ph = phien;                                     // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
            List<Object> ds = new ArrayList<>();
            long trong = 0;
            for (String k : cacO) {
                Map<String, Object> x = kho.doc(k);
                ds.add(x);
                if (x.get("khach") == null) trong++;
            }
            List<Object> ls;
            synchronized (lichSu) { ls = new ArrayList<>(lichSu.subList(0, Math.min(8, lichSu.size()))); }
            return Json.obj("phien", ph, "so_xe", (long) ds.size(), "trong", trong, "xe", ds, "lich_su", ls);
        }

        @Override
        protected String tomTat() {
            long da = 0;
            for (String k : cacO) if (kho.doc(k).get("khach") != null) da++;
            return "Đã đặt " + da + "/" + cacO.size() + " xe, " + ((SoKiemToanDatXe) so).soDat + " lượt báo đặt được";
        }
    }

    static boolean cung(Object a, Object b) { return Objects.equals(a, b); }
}
