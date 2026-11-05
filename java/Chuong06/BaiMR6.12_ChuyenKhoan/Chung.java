import java.io.BufferedInputStream;
import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintStream;
import java.math.BigDecimal;
import java.math.MathContext;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.LockSupport;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.12 – Khung dùng chung cho nhóm bài "tranh chấp tài nguyên" MR6.6–MR6.13 (bản Java).
 *
 * Tệp này GIỐNG NHAU ở các bài tranh chấp (chép nguyên, chỉ đổi dòng đầu); nghiệp vụ riêng nằm ở MayChu.java
 * (lớp con của MayChuCoSo, lớp con của SoKiemToan) và phần "NGHIỆP VỤ" của MoPhong.java. Gồm:
 *   1. Giao thức: guiJson, maHoa, docDong (mỗi dòng một JSON UTF-8 kết thúc "\n", tối đa 64 KB), tenHopLe,
 *      docLenhStdin ("@t lệnh"), docThamSo ("khoa=gia_tri").
 *   2. Đo đạc: phanVi (hạng gần nhất), ThongKe (đếm yêu cầu, độ trễ xử lý µs, yc/s).
 *   3. KhoTranhChap: kho các "ô" tài nguyên và BỐN cách xử lý tranh chấp khi n luồng cùng ghi:
 *         khong_khoa  đọc – xử lý – ghi không đồng bộ        -> tranh chấp (race condition), mất cập nhật
 *         khoa        khóa loại trừ (ReentrantLock) theo từng ô, giữ suốt đọc – xử lý – ghi
 *         lac_quan    đọc (giá trị, phiên bản) -> xử lý ngoài khóa -> so sánh-và-đổi (CAS) dưới khóa ngắn
 *                     -> phiên bản đã đổi thì thử lại (tối đa 5 lần, nghỉ ngẫu nhiên 0–1 ms)
 *         hang_doi    mọi yêu cầu ghi xếp vào LinkedBlockingQueue cho MỘT luồng xử lý (mô hình actor)
 *   4. SoKiemToan: sổ ghi mọi giao dịch được chấp nhận (khóa riêng) và đếm sai phạm theo luật của bài.
 *   5. MayChuCoSo: máy chủ TCP đa luồng (một luồng mỗi kết nối): chào, quản trị, ma_yc lặp lại, đẩy cap_nhat
 *      gộp tối đa 10 lần/giây; lớp con chỉ viết các hàm nghiệp vụ.
 *   6. Định dạng in: tien(), dongThongKe(), bangSoSanh() – in GIỐNG HỆT bản Python.
 *
 * Giao thức (giống hệt ở 5 ngôn ngữ):
 *   client -> server  {"loai":"chao","ten":"An","vai_tro":"nguoi_dung"|"quan_tri"[,"so_nguoi":m]}  (đầu tiên)
 *                     {"loai":"<thao_tac>","ma_yc":k,["nguoi":"u17"],...}   (yêu cầu nghiệp vụ)
 *                     {"loai":"che_do","ma_yc":k,"gia_tri":"khoa"} | {"loai":"tre","ma_yc":k,"ms":2}
 *                     {"loai":"dat_lai","ma_yc":k,...tham số} | {"loai":"thong_ke","ma_yc":k}  (chỉ quan_tri)
 *   server -> client  {"loai":"chao","ma":id,"che_do":"khoa","may_chu":"..."} rồi ngay một cap_nhat
 *                     {"loai":"kq","ma_yc":k,"ok":true|false,["ma_loi":"..","ly_do":".."],...}
 *                     {"loai":"cap_nhat","che_do":"khoa",...trạng thái nghiệp vụ}  (gộp ≤ 10 lần/s)
 *                     {"loai":"su_kien","noi_dung":"..."} ; {"loai":"thong_ke","ma_yc":k,...}
 *                     {"loai":"loi","ly_do":"..."} (sai giao thức)
 * Ánh xạ JSON (Json.java): đối tượng ↔ LinkedHashMap, số nguyên ↔ Long, số thực ↔ Double.
 */
final class Chung {
    private Chung() {}

    static final int GIOI_HAN_DONG = 64 * 1024;          // một dòng JSON tối đa 64 KB
    static final List<String> CHE_DO = List.of("khong_khoa", "khoa", "lac_quan", "hang_doi");
    static final Map<String, String> TEN_CHE_DO = new LinkedHashMap<>();
    static {
        TEN_CHE_DO.put("khong_khoa", "Không khóa (lỗi tranh chấp)");
        TEN_CHE_DO.put("khoa", "Khóa loại trừ (mutex)");
        TEN_CHE_DO.put("lac_quan", "Lạc quan (phiên bản + CAS)");
        TEN_CHE_DO.put("hang_doi", "Hàng đợi một luồng (actor)");
    }
    static final List<String> VAI_TRO = List.of("nguoi_dung", "quan_tri");
    static final int SO_LAN_THU = 5;                     // lac_quan: số lần thử tối đa
    static final double CHO_KHOA_TOI_DA_S = 2.0;         // khoa: chờ một khóa quá 2 s thì bỏ (nghi khóa chết)
    static final int NHO_KQ = 1024;                      // mỗi kết nối nhớ kết quả của 1024 ma_yc gần nhất
    static final int GIU_DO_TRE = 200_000;               // giữ tối đa 200 000 mẫu độ trễ cho phân vị
    static final double NHIP_DAY_S = 0.1;                // đẩy cap_nhat tối đa 10 lần/giây

    // ================================================================ đồng hồ, in
    static final long T0 = System.nanoTime();

    /** Đồng hồ đơn điệu (giây) – như time.monotonic() của Python. */
    static double bayGio() { return (System.nanoTime() - T0) / 1e9; }

    /** Ngủ s giây (dưới 1 ms vẫn chính xác nhờ LockSupport.parkNanos). */
    static void ngu(double s) { if (s > 0) LockSupport.parkNanos((long) (s * 1e9)); }

    static String f(String mau, Object... thamSo) { return String.format(Locale.ROOT, mau, thamSo); }

    /** stdout UTF-8, tự xả mỗi dòng. */
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);

    /** In "[tiền tố] text"; nhiều luồng cùng in: mỗi dòng in trọn vẹn (một khóa chung). */
    static void inDong(String tienTo, String text) { synchronized (OUT) { OUT.println("[" + tienTo + "] " + text); } }

    static void inTho(String text) { synchronized (OUT) { OUT.println(text); } }

    static void chayNen(String ten, Runnable r) { Thread.ofPlatform().daemon().name(ten).start(r); }

    // ================================================================ 1. Giao thức
    /** Bytes của một dòng – dùng khi gửi nhiều thông điệp trong MỘT lần ghi (pipelining). */
    static byte[] maHoa(Map<String, Object> obj) { return (Json.stringify(obj) + "\n").getBytes(StandardCharsets.UTF_8); }

    /** Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n' (người gọi giữ khóa gửi của socket). */
    static void guiJson(OutputStream out, Map<String, Object> obj) throws IOException {
        out.write(maHoa(obj));
        out.flush();
    }

    /** Đọc một dòng JSON (luồng vào đã bọc BufferedInputStream). Trả về Map, hoặc null khi kết nối đóng;
     *  dòng > 64 KB, bị cắt hoặc sai dạng -> IOException (như ValueError của Python). */
    @SuppressWarnings("unchecked")
    static Map<String, Object> docDong(InputStream in) throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream(128);
        int c;
        while ((c = in.read()) != -1) {
            if (c == '\n') break;
            b.write(c);
            if (b.size() > GIOI_HAN_DONG) throw new IOException("dòng quá dài hoặc bị cắt");
        }
        if (c == -1) {
            if (b.size() == 0) return null;
            throw new IOException("dòng quá dài hoặc bị cắt");
        }
        Object o;
        try {
            o = Json.parse(b.toString(StandardCharsets.UTF_8));
        } catch (RuntimeException e) {
            throw new IOException("JSON sai: " + e.getMessage());
        }
        if (!(o instanceof Map<?, ?>)) throw new IOException("thông điệp không phải đối tượng JSON");
        return (Map<String, Object>) o;
    }

    static InputStream boc(InputStream in) { return new BufferedInputStream(in, 1 << 16); }

    /** 1–16 ký tự in được, không khoảng trắng đầu/cuối (như ten_hop_le của Python). */
    static boolean tenHopLe(Object o) {
        if (!(o instanceof String ten)) return false;
        int n = ten.codePointCount(0, ten.length());
        if (n < 1 || n > 16 || !ten.strip().equals(ten)) return false;
        return ten.codePoints().allMatch(cp -> cp == ' ' || (!Character.isISOControl(cp) && !Character.isWhitespace(cp)
                && Character.getType(cp) != Character.FORMAT && Character.getType(cp) != Character.UNASSIGNED));
    }

    static boolean laSoNguyen(Object x) { return x instanceof Long || x instanceof Integer; }

    /** Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu). */
    static void docLenhStdin(double t0, Consumer<String> xuLy, Runnable ketThuc) {
        try (BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                String dong = line.strip();
                if (dong.startsWith("@")) {
                    String[] p = dong.split(" ", 2);
                    ngu(Double.parseDouble(p[0].substring(1)) - (bayGio() - t0));
                    dong = p.length > 1 ? p[1].strip() : "";
                }
                if (!dong.isEmpty()) xuLy.accept(dong);
            }
        } catch (IOException | NumberFormatException ignored) { }
        if (ketThuc != null) ketThuc.run();
    }

    /** ["thoi_gian_s=5", "ten_mon=Bình"] -> {thoi_gian_s=5, ten_mon=Bình} (Long, rồi Double nếu đọc được). */
    static Map<String, Object> docThamSo(List<String> ds) {
        Map<String, Object> kq = new LinkedHashMap<>();
        for (String muc : ds) {
            int i = muc.indexOf('=');
            if (i < 0) throw new IllegalArgumentException("cần dạng khóa=giá_trị: " + muc);
            kq.put(muc.substring(0, i), soHoacChu(muc.substring(i + 1)));
        }
        return kq;
    }

    static Object soHoacChu(String v) {
        try { return Long.parseLong(v.strip()); } catch (NumberFormatException ignored) { }
        try {
            if (v.strip().matches("[+-]?(\\d+\\.?\\d*|\\.\\d+)([eE][+-]?\\d+)?")) return Double.parseDouble(v.strip());
        } catch (NumberFormatException ignored) { }
        return v;
    }

    /** dict(cu, k=v, …) của Python: bản sao mới có thay vài trường (hàm xử lý THUẦN không sửa dữ liệu đọc). */
    static Map<String, Object> sua(Map<String, Object> cu, Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>(cu);
        for (int k = 0; k + 1 < kv.length; k += 2) m.put((String) kv[k], kv[k + 1]);
        return m;
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> map(Object o) { return (Map<String, Object>) o; }

    @SuppressWarnings("unchecked")
    static List<Object> list(Object o) { return (List<Object>) o; }

    static long so(Object o) { return o instanceof Number n ? n.longValue() : 0; }

    static double sod(Object o) { return o instanceof Number n ? n.doubleValue() : 0; }

    static boolean dung(Object o) { return Boolean.TRUE.equals(o); }

    // ================================================================ 2. Đo đạc
    /** Phân vị p (0–100) theo HẠNG GẦN NHẤT: phần tử thứ ⌈p/100 · n⌉ (đếm từ 1) của dãy đã sắp xếp; rỗng -> null. */
    static Double phanVi(double[] daSapXep, int n, double p) {
        if (n == 0) return null;
        return daSapXep[Math.max(1, (int) Math.ceil(p / 100 * n)) - 1];
    }

    static Double phanVi(List<Double> daSapXep, double p) {
        if (daSapXep.isEmpty()) return null;
        return daSapXep.get(Math.max(1, (int) Math.ceil(p / 100 * daSapXep.size())) - 1);
    }

    /** Số liệu của máy chủ từ lần đặt lại gần nhất. Mọi hàm an toàn đa luồng (synchronized trên chính đối tượng). */
    static final class ThongKe {
        static final String[] DEM = {"so_yc", "thanh_cong", "tu_choi", "xung_dot", "thu_lai", "het_lan_thu",
                "khoa_cho_lau", "hang_doi_max"};
        private final Map<String, Long> dem = new LinkedHashMap<>();
        private Map<String, Long> maLoi;
        private double[] tre;
        private int nTre;
        private double tDau, tCuoi;
        private boolean coDau, coCuoi;
        private long giay, demGiay, ycS;

        ThongKe() { datLai(); }

        synchronized void datLai() {
            for (String k : DEM) dem.put(k, 0L);
            maLoi = new LinkedHashMap<>();
            tre = new double[1024];
            nTre = 0;
            coDau = coCuoi = false;
            giay = (long) bayGio();
            demGiay = ycS = 0;
        }

        synchronized void cong(String ten, long n) { dem.merge(ten, n, Long::sum); }

        void cong(String ten) { cong(ten, 1); }

        synchronized void datMax(String ten, long x) { if (x > dem.get(ten)) dem.put(ten, x); }

        synchronized void batDauYc() {
            dem.merge("so_yc", 1L, Long::sum);
            if (!coDau) { tDau = bayGio(); coDau = true; }
        }

        void xongYc(Map<String, Object> kq, double treUs) {
            double bg = bayGio();
            synchronized (this) {
                if (dung(kq.get("ok"))) dem.merge("thanh_cong", 1L, Long::sum);
                else {
                    dem.merge("tu_choi", 1L, Long::sum);
                    Object m = kq.get("ma_loi");
                    maLoi.merge(m == null ? "khac" : m.toString(), 1L, Long::sum);
                }
                if (nTre < GIU_DO_TRE) {
                    if (nTre == tre.length) tre = Arrays.copyOf(tre, Math.min(GIU_DO_TRE, tre.length * 2));
                    tre[nTre++] = treUs;
                }
                tCuoi = bg;
                coCuoi = true;
                sangGiay(bg);
                demGiay++;
            }
        }

        /** yc_s = số yêu cầu xong trong giây (đồng hồ) liền trước. */
        private void sangGiay(double bg) {
            long g = (long) bg;
            if (g != giay) {
                ycS = g == giay + 1 ? demGiay : 0;
                giay = g;
                demGiay = 0;
            }
        }

        synchronized Map<String, Object> anhChup() {
            sangGiay(bayGio());
            Map<String, Object> d = new LinkedHashMap<>(dem);
            double[] s = Arrays.copyOf(tre, nTre);
            Arrays.sort(s);
            d.put("ma_loi", new LinkedHashMap<>(maLoi));
            d.put("yc_s", ycS);
            long xong = dem.get("thanh_cong") + dem.get("tu_choi");
            double khoang = coDau && coCuoi ? tCuoi - tDau : 0;
            d.put("thong_luong", khoang > 0 ? Math.round(xong / khoang) : 0L);
            for (int p : new int[] {50, 95, 99}) {
                Double v = phanVi(s, s.length, p);
                d.put("p" + p + "_us", v == null ? null : (Object) Math.round(v));
            }
            return d;
        }
    }

    // ================================================================ 3. Kho tài nguyên + 4 chế độ
    /** Kết quả của hàm nghiệp vụ THUẦN: kq trả lời client, ghi = {khóa ô: GIÁ TRỊ MỚI} hoặc null (không ghi). */
    record Kq(Map<String, Object> kq, Map<String, Object> ghi) {}

    @FunctionalInterface
    interface XuLy { Kq apply(Map<String, Object> doc); }

    /** Một ô tài nguyên (món hàng, ghế, tài khoản…): giá trị, phiên bản (cho lac_quan), khóa riêng (cho khoa). */
    static final class O {
        volatile Object giaTri;
        volatile long phienBan;
        final ReentrantLock khoa = new ReentrantLock();

        O(Object v) { giaTri = v; }
    }

    /**
     * Kho các ô tài nguyên; mọi thao tác GHI đi qua thucHien() và được xử lý theo chế độ hiện tại.
     *
     * Nghiệp vụ viết hàm THUẦN  xuLy(doc) -> Kq(kq, ghi): doc = {khóa ô: giá trị đọc được}; kq = Map trả lời
     * client; ghi = null hoặc {khóa ô: giá trị MỚI} – KHÔNG sửa doc tại chỗ (dùng Chung.sua) vì lac_quan có thể
     * gọi xuLy nhiều lần. khiGhi(kq) được gọi NGAY SAU khi ghi, vẫn trong vùng găng của chế độ: nơi ghi sổ kiểm
     * toán đúng thứ tự. traLoi(kq) được gọi đúng một lần khi xong.
     *
     * "Cổng" (tamDung): đổi chế độ / đặt lại chỉ diễn ra khi không còn thao tác nào đang chạy hay đang xếp hàng.
     */
    static final class KhoTranhChap {
        final ThongKe thongKe;
        volatile String cheDo;
        volatile double treMs;
        volatile boolean khoaNgayTho;                  // khoa: true = khóa theo thứ tự cacKhoa (có thể khóa chết)
        private volatile Map<String, O> o = new ConcurrentHashMap<>();
        private final Object khoaCas = new Object();   // lac_quan: khóa rất ngắn quanh ảnh chụp và so sánh-và-đổi
        private final Object cong = new Object();
        private boolean dongCong;
        private int dangChay;
        private final LinkedBlockingQueue<Viec> hang = new LinkedBlockingQueue<>();

        private record Viec(List<String> cacKhoa, XuLy xuLy, Consumer<Map<String, Object>> khiGhi,
                            Consumer<Map<String, Object>> traLoi) {}

        private static final Viec DUNG = new Viec(null, null, null, null);

        KhoTranhChap(ThongKe thongKe, String cheDo, double treMs) {
            if (!CHE_DO.contains(cheDo)) throw new IllegalArgumentException("chế độ không hợp lệ: " + cheDo);
            this.thongKe = thongKe;
            this.cheDo = cheDo;
            this.treMs = treMs;
            chayNen("hang_doi", this::vongHangDoi);
        }

        // ------------------------------------------------------------ ô tài nguyên
        void datO(String khoa, Object giaTri) { o.put(khoa, new O(giaTri)); }

        void xoaHet() { o = new ConcurrentHashMap<>(); }

        /** Đọc không khóa (để hiển thị): giá trị được thay nguyên đối tượng nên luôn là một bản trọn vẹn. */
        @SuppressWarnings("unchecked")
        Map<String, Object> doc(String khoa) { return (Map<String, Object>) o.get(khoa).giaTri; }

        Map<String, Object> anhChup() {
            Map<String, Object> m = new LinkedHashMap<>();
            for (Map.Entry<String, O> e : o.entrySet()) m.put(e.getKey(), e.getValue().giaTri);
            return m;
        }

        int doDaiHang() { return hang.size(); }

        // ------------------------------------------------------------ cấu hình
        /** Đóng cổng, chờ mọi thao tác đang chạy/đang xếp hàng xong; close() mở lại (dùng với try-with-resources). */
        AutoCloseable tamDung() {
            synchronized (cong) {
                while (dongCong) cho(cong);
                dongCong = true;
                while (dangChay > 0) cho(cong);
            }
            return () -> { synchronized (cong) { dongCong = false; cong.notifyAll(); } };
        }

        private static void cho(Object m) {
            try { m.wait(); } catch (InterruptedException e) { Thread.currentThread().interrupt(); throw new IllegalStateException(e); }
        }

        void datCheDo(Object cd) {
            if (!(cd instanceof String s) || !CHE_DO.contains(s))
                throw new IllegalArgumentException("chế độ không hợp lệ: " + py(cd));
            chayDung(() -> cheDo = s);
        }

        /** Chạy r khi cổng đóng (không thao tác nào chạy lẫn). */
        void chayDung(Runnable r) {
            try (AutoCloseable c = tamDung()) {
                r.run();
            } catch (RuntimeException e) {
                throw e;
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        }

        void datTre(Object ms) {
            if (!(ms instanceof Number n)) throw new IllegalArgumentException("độ trễ phải là số: " + py(ms));
            double x = n.doubleValue();
            if (!(x >= 0 && x <= 100)) throw new IllegalArgumentException("độ trễ 0–100 ms");
            treMs = x;
        }

        /** Giả lập thời gian xử lý (đọc cơ sở dữ liệu, kiểm tra nghiệp vụ): ngẫu nhiên 0 … treMs. */
        private void tre() {
            double t = treMs;
            if (t > 0) ngu(ThreadLocalRandom.current().nextDouble(0, t) / 1000);
        }

        // ------------------------------------------------------------ thực hiện một thao tác
        void thucHien(List<String> dsKhoa, XuLy xuLy, Consumer<Map<String, Object>> traLoi,
                      Consumer<Map<String, Object>> khiGhi) {
            List<String> cacKhoa = new ArrayList<>(new LinkedHashSet<>(dsKhoa));     // bỏ trùng, giữ thứ tự
            Map<String, O> om = o;
            for (String k : cacKhoa)
                if (!om.containsKey(k)) {
                    traLoi.accept(Json.obj("ok", false, "ma_loi", "khong_co", "ly_do", "không có tài nguyên"));
                    return;
                }
            String cd;
            synchronized (cong) {
                while (dongCong) cho(cong);
                dangChay++;
                cd = cheDo;
            }
            if (cd.equals("hang_doi")) {
                hang.add(new Viec(cacKhoa, xuLy, khiGhi, traLoi));
                thongKe.datMax("hang_doi_max", hang.size());
                return;                                            // luồng xử lý sẽ gọi traLoi
            }
            Map<String, Object> kq;
            try {
                kq = switch (cd) {
                    case "khong_khoa" -> motLan(cacKhoa, xuLy, khiGhi);
                    case "khoa" -> coKhoa(cacKhoa, xuLy, khiGhi);
                    default -> lacQuan(cacKhoa, xuLy, khiGhi);
                };
            } catch (RuntimeException e) {                          // lỗi nghiệp vụ: trả lời lỗi, không sập luồng
                kq = loiXuLy(e);
            } finally {
                xongMot();
            }
            traLoi.accept(kq);
        }

        private static Map<String, Object> loiXuLy(RuntimeException e) {
            return Json.obj("ok", false, "ma_loi", "loi", "ly_do", "lỗi xử lý: " + e);
        }

        private void xongMot() {
            synchronized (cong) {
                dangChay--;
                if (dangChay == 0) cong.notifyAll();
            }
        }

        private void ghi(Map<String, Object> ghi, Map<String, Object> kq, Consumer<Map<String, Object>> khiGhi) {
            for (Map.Entry<String, Object> e : ghi.entrySet()) {
                O x = o.get(e.getKey());
                x.giaTri = e.getValue();
                x.phienBan++;
            }
            if (khiGhi != null) khiGhi.accept(kq);
        }

        /** khong_khoa (và luồng hang_doi): đọc -> trễ -> xử lý -> ghi, KHÔNG khóa. Hai luồng cùng đọc giá trị
         *  cũ rồi cùng ghi: bản ghi sau đè bản trước (mất cập nhật). */
        private Map<String, Object> motLan(List<String> cacKhoa, XuLy xuLy, Consumer<Map<String, Object>> khiGhi) {
            Map<String, Object> doc = new LinkedHashMap<>();
            for (String k : cacKhoa) doc.put(k, o.get(k).giaTri);
            tre();
            Kq r = xuLy.apply(doc);
            if (r.ghi() != null && !r.ghi().isEmpty()) ghi(r.ghi(), r.kq(), khiGhi);
            return r.kq();
        }

        /** khoa: giữ khóa của MỌI ô liên quan suốt đọc – xử lý – ghi (vùng găng – critical section). Khóa theo thứ
         *  tự tên ô đã sắp xếp => mọi luồng xin khóa cùng một thứ tự => không khóa chết. khoaNgayTho: khóa theo
         *  thứ tự yêu cầu (ví dụ nguồn rồi đích), nghỉ giữa hai lần khóa. */
        private Map<String, Object> coKhoa(List<String> cacKhoa, XuLy xuLy, Consumer<Map<String, Object>> khiGhi) {
            boolean ngayTho = khoaNgayTho;
            List<String> thuTu = new ArrayList<>(cacKhoa);
            if (!ngayTho) thuTu.sort(null);
            List<ReentrantLock> daGiu = new ArrayList<>();
            try {
                for (int i = 0; i < thuTu.size(); i++) {
                    String k = thuTu.get(i);
                    if (i > 0 && ngayTho) tre();
                    ReentrantLock l = o.get(k).khoa;
                    boolean duoc;
                    try {
                        duoc = l.tryLock((long) (CHO_KHOA_TOI_DA_S * 1000), TimeUnit.MILLISECONDS);
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        duoc = false;
                    }
                    if (!duoc) {
                        thongKe.cong("khoa_cho_lau");
                        return Json.obj("ok", false, "ma_loi", "khoa_cho_lau",
                                "ly_do", "chờ khóa " + k + " quá " + g(CHO_KHOA_TOI_DA_S) + " s (khóa chết?)");
                    }
                    daGiu.add(l);
                }
                return motLan(cacKhoa, xuLy, khiGhi);
            } finally {
                for (int i = daGiu.size() - 1; i >= 0; i--) daGiu.get(i).unlock();
            }
        }

        /** lac_quan (Kung & Robinson 1981): không giữ khóa khi xử lý; lúc ghi kiểm tra phiên bản. */
        private Map<String, Object> lacQuan(List<String> cacKhoa, XuLy xuLy, Consumer<Map<String, Object>> khiGhi) {
            for (int lan = 0; lan < SO_LAN_THU; lan++) {
                Map<String, Object> doc = new LinkedHashMap<>();
                long[] phienBan = new long[cacKhoa.size()];
                synchronized (khoaCas) {                               // ảnh chụp nhất quán của các ô
                    for (int i = 0; i < cacKhoa.size(); i++) {
                        O x = o.get(cacKhoa.get(i));
                        doc.put(cacKhoa.get(i), x.giaTri);
                        phienBan[i] = x.phienBan;
                    }
                }
                tre();
                Kq r = xuLy.apply(doc);
                if (r.ghi() == null || r.ghi().isEmpty()) return r.kq();   // chỉ đọc / từ chối: không cần CAS
                synchronized (khoaCas) {                               // so sánh-và-đổi (compare-and-swap)
                    boolean khop = true;
                    for (int i = 0; i < cacKhoa.size(); i++) khop &= o.get(cacKhoa.get(i)).phienBan == phienBan[i];
                    if (khop) {
                        ghi(r.ghi(), r.kq(), khiGhi);
                        return r.kq();
                    }
                }
                thongKe.cong("xung_dot");                              // có luồng khác ghi trước: làm lại
                if (lan < SO_LAN_THU - 1) {
                    thongKe.cong("thu_lai");
                    ngu(ThreadLocalRandom.current().nextDouble(0, 0.001));
                }
            }
            thongKe.cong("het_lan_thu");
            return Json.obj("ok", false, "ma_loi", "xung_dot", "ly_do", "xung đột, thử lại sau");
        }

        /** hang_doi: MỘT luồng lấy lần lượt từng việc (đến trước phục vụ trước) – không cần khóa vì chỉ có luồng
         *  này chạm vào trạng thái. */
        private void vongHangDoi() {
            while (true) {
                Viec v;
                try { v = hang.take(); } catch (InterruptedException e) { return; }
                if (v == DUNG) return;
                Map<String, Object> kq;
                try {
                    kq = motLan(v.cacKhoa(), v.xuLy(), v.khiGhi());
                } catch (RuntimeException e) {
                    kq = loiXuLy(e);
                }
                xongMot();
                try { v.traLoi().accept(kq); } catch (RuntimeException ignored) { }
            }
        }

        void dong() { hang.add(DUNG); }
    }

    // ================================================================ 4. Sổ kiểm toán
    /** Ghi mọi giao dịch ĐƯỢC CHẤP NHẬN (dưới khóa riêng của sổ – không ảnh hưởng tranh chấp của kho) và đếm sai
     *  phạm. Lớp con truyền tenSaiPham và viết kiemTra(gd) (gọi khi ghi từng giao dịch, theo thứ tự ghi sổ) và
     *  kiemTraCuoi(doc) (đối chiếu sổ với trạng thái hiện tại của kho, trả về {mã: số}). */
    static class SoKiemToan {
        final Map<String, String> tenSaiPham;              // mã -> tên ngắn
        final List<Map<String, Object>> giaoDich = new ArrayList<>();
        final Map<String, Long> dem = new LinkedHashMap<>();
        final List<String> viDu = new ArrayList<>();       // mô tả vài sai phạm đầu tiên

        SoKiemToan(Map<String, String> tenSaiPham) {
            this.tenSaiPham = tenSaiPham;
            for (String m : tenSaiPham.keySet()) dem.put(m, 0L);
        }

        final synchronized void ghi(Map<String, Object> gd) {
            giaoDich.add(gd);
            kiemTra(gd);
        }

        protected void saiPham(String ma, String moTa) {
            dem.merge(ma, 1L, Long::sum);
            if (viDu.size() < 5) viDu.add(moTa);
        }

        protected void kiemTra(Map<String, Object> gd) {}

        protected Map<String, Long> kiemTraCuoi(Map<String, Object> doc) { return Map.of(); }

        record TongKet(List<Map<String, Object>> ds, long tong, List<String> viDu) {}

        synchronized TongKet tongKet(Map<String, Object> doc) {
            Map<String, Long> d = new LinkedHashMap<>(dem);
            for (Map.Entry<String, Long> e : kiemTraCuoi(doc).entrySet()) d.merge(e.getKey(), e.getValue(), Long::sum);
            List<Map<String, Object>> ds = new ArrayList<>();
            long tong = 0;
            for (Map.Entry<String, String> e : tenSaiPham.entrySet()) {
                long s = d.get(e.getKey());
                ds.add(Json.obj("ma", e.getKey(), "ten", e.getValue(), "so", s));
                tong += s;
            }
            return new TongKet(ds, tong, new ArrayList<>(viDu));
        }
    }

    // ================================================================ 5. Máy chủ TCP đa luồng
    /** Một kết nối tới máy chủ (người dùng thật, kết nối gộp của bộ mô phỏng, hoặc quản trị). */
    static final class KhachKetNoi {
        final long ma;
        final String ten, vaiTro;
        final Socket conn;
        final OutputStream out;
        final Long soNguoi;                                // null: người dùng thật; m: kết nối gộp m người ảo
        final Object khoaGui = new Object();               // nhiều luồng (kết nối, hàng đợi, đẩy) cùng gửi
        private final LinkedHashMap<Long, Map<String, Object>> daTra = new LinkedHashMap<>();  // null: đang xử lý
        long soYc;

        KhachKetNoi(long ma, String ten, String vaiTro, Socket conn, Long soNguoi) throws IOException {
            this.ma = ma;
            this.ten = ten;
            this.vaiTro = vaiTro;
            this.conn = conn;
            this.out = conn.getOutputStream();
            this.soNguoi = soNguoi;
        }

        boolean gui(Map<String, Object> obj) {
            try {
                synchronized (khoaGui) { guiJson(out, obj); }
                return true;
            } catch (IOException e) {
                return false;
            }
        }

        /** true nếu ma_yc mới. ma_yc đã gặp: gửi lại kết quả cũ (nếu đã có) kèm "lap_lai": true, trả về false. */
        boolean nhanYc(long maYc) {
            synchronized (khoaGui) {
                if (daTra.containsKey(maYc)) {
                    Map<String, Object> cu = daTra.get(maYc);
                    if (cu != null) {
                        try { guiJson(out, sua(cu, "lap_lai", true)); } catch (IOException ignored) { }
                    }
                    return false;
                }
                daTra.put(maYc, null);
                if (daTra.size() > NHO_KQ) {
                    Iterator<Long> it = daTra.keySet().iterator();
                    it.next();
                    it.remove();
                }
                return true;
            }
        }

        void tra(long maYc, Map<String, Object> tin) {
            synchronized (khoaGui) {
                daTra.put(maYc, tin);
                try { guiJson(out, tin); } catch (IOException ignored) { }
            }
        }
    }

    /**
     * Khung máy chủ. Lớp con (MayChu.java) truyền tên, các loại yêu cầu, tham số mặc định cho constructor rồi gọi
     * khoiDong(thamSo) ở CUỐI constructor của mình, và viết:
     *   khoiTao()                                  tạo các ô trong kho, tạo so (SoKiemToan)
     *   xuLyYeuCau(khach, nguoi, tin, traLoi)      kiểm tra tin, gọi kho.thucHien(...)
     *   trangThaiNghiepVu()                        Map trạng thái nghiệp vụ cho cap_nhat (đọc không khóa)
     *   tomTat()                                   một dòng ngắn (≤ 48 ký tự) mô tả kết quả
     *   dinhKy(bayGio) (tùy chọn)                  gọi mỗi 100 ms trên luồng nền (đồng hồ, hết hạn…)
     * Báo đổi trạng thái: baoDoi() (đẩy cap_nhat ở nhịp kế tiếp); thông báo: thongBao(text).
     * Lưu ý Java: lớp con KHÔNG khởi tạo trường ở chỗ khai báo cho những gì khoiTao() đặt.
     */
    abstract static class MayChuCoSo {
        final String TEN;
        final List<String> LOAI_YEU_CAU;
        final Map<String, Object> THAM_SO_MAC_DINH;
        final Consumer<String> bao;
        final ThongKe thongKe = new ThongKe();
        final KhoTranhChap kho;
        final Map<String, Object> thamSo;
        private final Map<Long, KhachKetNoi> ds = new LinkedHashMap<>();
        volatile boolean chay;
        volatile String diaChi = "";
        private volatile boolean doi;
        private final ConcurrentLinkedQueue<String> suKien = new ConcurrentLinkedQueue<>();
        private final List<Thread> luong = new ArrayList<>();
        private ServerSocket srv;
        private final AtomicLong demMa = new AtomicLong();
        SoKiemToan so;

        MayChuCoSo(String ten, List<String> loai, Map<String, Object> macDinh, String cheDo, double treMs,
                   Consumer<String> bao) {
            TEN = ten;
            LOAI_YEU_CAU = loai;
            THAM_SO_MAC_DINH = macDinh;
            this.bao = bao != null ? bao : text -> { };
            kho = new KhoTranhChap(thongKe, cheDo, treMs);
            thamSo = new LinkedHashMap<>(macDinh);
        }

        /** Gọi ở cuối constructor của lớp con: kiểm tra, áp tham số rồi khởi tạo trạng thái. */
        protected final void khoiDong(Map<String, Object> ts) {
            Map<String, Object> ep = epKieu(ts == null ? Map.of() : ts);
            kiemThamSo(ep);
            thamSo.putAll(ep);
            khoiTaoLai();
        }

        // ------------------------------------------------------------ phần nghiệp vụ (lớp con viết)
        protected abstract void khoiTao();

        protected abstract void xuLyYeuCau(KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                           Consumer<Map<String, Object>> traLoi);

        protected Map<String, Object> trangThaiNghiepVu() { return new LinkedHashMap<>(); }

        protected String tomTat() { return ""; }

        protected void dinhKy(double bayGio) {}

        /** Kiểm tra tham số đặt lại; lớp con mở rộng (ném IllegalArgumentException). */
        protected void kiemThamSo(Map<String, Object> ts) {
            for (String k : ts.keySet())
                if (!THAM_SO_MAC_DINH.containsKey(k)) throw new IllegalArgumentException("tham số lạ: " + k);
        }

        double tsD(String k) { return sod(thamSo.get(k)); }

        long tsL(String k) { return so(thamSo.get(k)); }

        String tsS(String k) { return String.valueOf(thamSo.get(k)); }

        // ------------------------------------------------------------ điều khiển
        private void khoiTaoLai() {
            kho.chayDung(() -> {                                    // không còn thao tác nào đang chạy
                kho.xoaHet();
                thongKe.datLai();
                khoiTao();
            });
            baoDoi();
        }

        /** Ép kiểu theo giá trị mặc định (như type(mặc định)(v) của Python). */
        private Map<String, Object> epKieu(Map<String, Object> thamSoVao) {
            Map<String, Object> ts = new LinkedHashMap<>();
            for (Map.Entry<String, Object> e : thamSoVao.entrySet()) {
                String k = e.getKey();
                if (!THAM_SO_MAC_DINH.containsKey(k)) throw new IllegalArgumentException("tham số lạ: " + k);
                Object md = THAM_SO_MAC_DINH.get(k), v = e.getValue();
                if (md instanceof Long) {
                    if (v instanceof Number n) ts.put(k, n.longValue());
                    else if (v instanceof Boolean b) ts.put(k, b ? 1L : 0L);
                    else {
                        try { ts.put(k, Long.parseLong(String.valueOf(v).strip())); } catch (NumberFormatException x) {
                            throw new IllegalArgumentException("invalid literal for int() with base 10: '" + py(v) + "'");
                        }
                    }
                } else if (md instanceof Double) {
                    if (v instanceof Number n) ts.put(k, n.doubleValue());
                    else if (v instanceof Boolean b) ts.put(k, b ? 1.0 : 0.0);
                    else {
                        Object x = v instanceof String s ? soHoacChu(s) : null;
                        if (!(x instanceof Number n)) throw new IllegalArgumentException("could not convert string to float: '" + py(v) + "'");
                        ts.put(k, n.doubleValue());
                    }
                } else ts.put(k, py(v));
            }
            return ts;
        }

        void datLai(Map<String, Object> thamSoVao) {
            Map<String, Object> ts = epKieu(thamSoVao);
            kiemThamSo(ts);
            Map<String, Object> tk = thongKe.anhChup();
            if (so(tk.get("so_yc")) > 0) {
                long sp = so.tongKet(kho.anhChup()).tong();
                bao.accept("Lượt trước: " + kho.cheDo + ", " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong")
                        + ", sai phạm " + sp);
            }
            thamSo.putAll(ts);
            khoiTaoLai();
            List<String> p = new ArrayList<>();
            for (Map.Entry<String, Object> e : ts.entrySet()) p.add(e.getKey() + "=" + py(e.getValue()));
            thongBao("Đặt lại" + (ts.isEmpty() ? "" : " (" + String.join(", ", p) + ")"));
        }

        void datCheDo(Object cheDo) {
            if (cheDo != null && cheDo.equals(kho.cheDo)) return;
            kho.datCheDo(cheDo);
            thongBao("Chế độ: " + cheDo + " – " + TEN_CHE_DO.get(cheDo));
            baoDoi();
        }

        void datTre(Object ms) {
            kho.datTre(ms);
            bao.accept("Độ trễ xử lý: ngẫu nhiên 0–" + g(kho.treMs) + " ms");
        }

        void baoDoi() { doi = true; }

        /** Ghi nhật ký máy chủ và gửi {"loai":"su_kien"} cho mọi client (ở nhịp đẩy kế tiếp). */
        void thongBao(String text) {
            bao.accept(text);
            suKien.add(text);
        }

        // ------------------------------------------------------------ khởi động / dừng
        synchronized void batDau(String host, int cong) throws IOException {
            ServerSocket s = new ServerSocket();
            s.setReuseAddress(true);
            s.bind(new InetSocketAddress(InetAddress.getByName(host), cong), 1024);
            s.setSoTimeout(500);
            srv = s;
            diaChi = host + ":" + cong;
            chay = true;
            luong.clear();
            luong.add(Thread.ofPlatform().daemon().name("nhan").start(this::nhanKetNoi));
            luong.add(Thread.ofPlatform().daemon().name("nen").start(this::vongNen));
            bao.accept("Lắng nghe " + host + ":" + cong + "; chế độ " + kho.cheDo + ", trễ 0–" + g(kho.treMs) + " ms");
        }

        synchronized void dung() {
            if (!chay) return;
            chay = false;
            List<KhachKetNoi> cac;
            synchronized (ds) { cac = new ArrayList<>(ds.values()); }
            for (KhachKetNoi k : cac) {
                try { k.conn.shutdownInput(); } catch (IOException ignored) { }
                try { k.conn.shutdownOutput(); } catch (IOException ignored) { }
            }
            for (Thread t : luong) {
                try { t.join(2000); } catch (InterruptedException ignored) { }
            }
            try { srv.close(); } catch (IOException ignored) { }
            bao.accept("Máy chủ đã dừng");
        }

        // ------------------------------------------------------------ kết nối
        private void nhanKetNoi() {
            while (chay) {
                Socket c;
                try {
                    c = srv.accept();
                } catch (SocketTimeoutException e) {
                    continue;
                } catch (IOException e) {
                    break;
                }
                Thread.ofPlatform().daemon().name("ket_noi").start(() -> phucVu(c));
            }
        }

        /** Luồng của một kết nối: chào, rồi đọc và xử lý lần lượt từng yêu cầu (có thể pipelining). */
        private void phucVu(Socket conn) {
            KhachKetNoi k = null;
            try {
                conn.setTcpNoDelay(true);
                InputStream in = boc(conn.getInputStream());
                Map<String, Object> tin = docDong(in);
                Object soNguoi = tin != null ? tin.get("so_nguoi") : null;
                Object vaiTro = tin != null && tin.containsKey("vai_tro") ? tin.get("vai_tro") : "nguoi_dung";
                if (tin == null || !"chao".equals(tin.get("loai")) || !tenHopLe(tin.get("ten"))
                        || !VAI_TRO.contains(vaiTro) || (soNguoi != null && !(laSoNguyen(soNguoi) && so(soNguoi) > 0))) {
                    guiJson(conn.getOutputStream(), Json.obj("loai", "loi", "ly_do", "cần chào: tên 1–16 ký tự, vai_tro hợp lệ"));
                    return;
                }
                k = new KhachKetNoi(demMa.incrementAndGet(), (String) tin.get("ten"), (String) vaiTro, conn,
                        soNguoi == null ? null : so(soNguoi));
                synchronized (k.khoaGui) {                        // luồng đẩy chưa gửi được gì cho k trước lời chào
                    synchronized (ds) { ds.put(k.ma, k); }
                    guiJson(k.out, Json.obj("loai", "chao", "ma", k.ma, "che_do", kho.cheDo, "may_chu", TEN));
                    guiJson(k.out, goiCapNhat());
                }
                if (k.soNguoi == null && k.vaiTro.equals("nguoi_dung")) bao.accept(k.ten + " vào");   // gộp, quản trị: không in
                while (chay) {
                    tin = docDong(in);
                    if (tin == null) break;
                    nhanTin(k, tin);
                }
            } catch (IOException e) {
                if (k != null && chay && k.soNguoi == null) bao.accept(k.ten + ": lỗi kết nối (" + e.getMessage() + ")");
            } finally {
                if (k != null) {
                    synchronized (ds) { ds.remove(k.ma); }
                    if (k.soNguoi == null && k.vaiTro.equals("nguoi_dung") && chay) bao.accept(k.ten + " rời");
                }
                try { conn.close(); } catch (IOException ignored) { }
            }
        }

        private void nhanTin(KhachKetNoi k, Map<String, Object> tin) {
            Object loai = tin.get("loai"), ma = tin.get("ma_yc");
            if (!laSoNguyen(ma)) {
                k.gui(Json.obj("loai", "loi", "ly_do", "thiếu ma_yc (số nguyên)"));
                return;
            }
            long maYc = so(ma);
            if (!k.nhanYc(maYc)) return;                               // ma_yc lặp lại: đã gửi lại kết quả cũ
            k.soYc++;
            if (LOAI_YEU_CAU.contains(loai)) {
                Object nguoi = k.soNguoi != null ? tin.get("nguoi") : k.ten;
                if (!tenHopLe(nguoi)) {
                    k.tra(maYc, Json.obj("loai", "kq", "ma_yc", maYc, "ok", false, "ma_loi", "sai_du_lieu",
                            "ly_do", "thiếu tên người (nguoi)"));
                    return;
                }
                long t0 = System.nanoTime();
                thongKe.batDauYc();
                final KhachKetNoi kk = k;
                Consumer<Map<String, Object>> traLoi = kq -> {             // từ luồng kết nối hoặc luồng hàng đợi
                    thongKe.xongYc(kq, (System.nanoTime() - t0) / 1e3);
                    Map<String, Object> m = Json.obj("loai", "kq", "ma_yc", maYc);
                    m.putAll(kq);
                    kk.tra(maYc, m);
                };
                xuLyYeuCau(k, (String) nguoi, tin, traLoi);
            } else if (List.of("che_do", "tre", "dat_lai", "thong_ke").contains(loai)) {
                if (!k.vaiTro.equals("quan_tri")) {
                    k.tra(maYc, Json.obj("loai", "kq", "ma_yc", maYc, "ok", false, "ma_loi", "khong_quyen",
                            "ly_do", "chỉ quản trị"));
                    return;
                }
                try {
                    if (loai.equals("thong_ke")) {
                        Map<String, Object> m = Json.obj("loai", "thong_ke", "ma_yc", maYc);
                        m.putAll(thongKeDayDu());
                        k.tra(maYc, m);
                        return;
                    }
                    if (loai.equals("che_do")) datCheDo(tin.get("gia_tri"));
                    else if (loai.equals("tre")) datTre(tin.get("ms"));
                    else {
                        Map<String, Object> ts = new LinkedHashMap<>(tin);
                        ts.remove("loai");
                        ts.remove("ma_yc");
                        datLai(ts);
                    }
                    k.tra(maYc, Json.obj("loai", "kq", "ma_yc", maYc, "ok", true));
                } catch (IllegalArgumentException | ClassCastException e) {
                    k.tra(maYc, Json.obj("loai", "kq", "ma_yc", maYc, "ok", false, "ma_loi", "sai_du_lieu",
                            "ly_do", String.valueOf(e.getMessage())));
                }
            } else {
                k.tra(maYc, Json.obj("loai", "kq", "ma_yc", maYc, "ok", false, "ma_loi", "khong_ho_tro",
                        "ly_do", "loại không hỗ trợ: " + py(loai)));
            }
        }

        // ------------------------------------------------------------ đẩy trạng thái (gộp ≤ 10 lần/giây)
        private Map<String, Object> goiCapNhat() {
            Map<String, Object> m = Json.obj("loai", "cap_nhat", "che_do", kho.cheDo);
            m.putAll(trangThaiNghiepVu());
            return m;
        }

        private void vongNen() {
            while (chay) {
                ngu(NHIP_DAY_S);
                try {
                    dinhKy(bayGio());
                } catch (RuntimeException e) {
                    bao.accept("lỗi định kỳ: " + e);
                }
                ByteArrayOutputStream goi = new ByteArrayOutputStream();
                String s;
                while ((s = suKien.poll()) != null) goi.writeBytes(maHoa(Json.obj("loai", "su_kien", "noi_dung", s)));
                if (doi) {
                    doi = false;                                       // xóa cờ TRƯỚC khi chụp: đổi sau đó vẫn được đẩy
                    goi.writeBytes(maHoa(goiCapNhat()));
                }
                if (goi.size() == 0) continue;
                byte[] duLieu = goi.toByteArray();
                List<KhachKetNoi> cac;
                synchronized (ds) { cac = new ArrayList<>(ds.values()); }
                for (KhachKetNoi k : cac) {
                    try {
                        synchronized (k.khoaGui) { k.out.write(duLieu); k.out.flush(); }
                    } catch (IOException ignored) { }
                }
            }
        }

        // ------------------------------------------------------------ số liệu
        /** Trạng thái hiện tại (như cap_nhat gửi client) – giao diện máy chủ đọc định kỳ. */
        Map<String, Object> trangThai() { return goiCapNhat(); }

        long[] soKetNoi() {
            List<KhachKetNoi> cac;
            synchronized (ds) { cac = new ArrayList<>(ds.values()); }
            long nguoi = 0;
            for (KhachKetNoi k : cac) if (!k.vaiTro.equals("quan_tri")) nguoi += k.soNguoi == null ? 1 : k.soNguoi;
            return new long[] {cac.size(), nguoi};
        }

        Map<String, Object> thongKeDayDu() {
            Map<String, Object> tk = thongKe.anhChup();
            SoKiemToan.TongKet t = so.tongKet(kho.anhChup());
            long[] kn = soKetNoi();
            Map<String, Object> m = Json.obj("che_do", kho.cheDo, "tre_ms", kho.treMs);
            m.putAll(tk);
            m.put("hang_doi", (long) kho.doDaiHang());
            m.put("sai_pham", t.ds());
            m.put("sai_pham_tong", t.tong());
            m.put("vi_du_sai_pham", new ArrayList<>(t.viDu().subList(0, Math.min(3, t.viDu().size()))));
            m.put("tom_tat", tomTat());
            m.put("so_ket_noi", kn[0]);
            m.put("so_nguoi", kn[1]);
            return m;
        }
    }

    // ================================================================ 6. Định dạng in (giống nhau ở 5 ngôn ngữ)
    /** 5300000 -> "5.300.000 đ" (dấu chấm ngăn nghìn). */
    static String tien(long x) { return f("%,d", x).replace(',', '.') + " đ"; }

    /** str() của Python: None, True/False, số nguyên không có ".0", số thực theo repr ("5.0", "0.5"). */
    static String py(Object v) {
        if (v == null) return "None";
        if (v instanceof Boolean b) return b ? "True" : "False";
        if (v instanceof Map<?, ?> || v instanceof List<?>) return Json.stringify(v);
        return v.toString();
    }

    /** Định dạng "%g" của Python (6 chữ số có nghĩa, bỏ số 0 thừa): 2.0 -> "2", 0.5 -> "0.5". */
    static String g(double x) {
        if (x == Math.rint(x) && Math.abs(x) < 1e6) return String.valueOf((long) x);
        return new BigDecimal(x).round(new MathContext(6)).stripTrailingZeros().toPlainString();
    }

    static String soUs(Object x) { return x == null ? "–" : py(x); }

    /** "Sai phạm 37: giá thấp hơn 36, giá cuối sai 1". */
    static String dongSaiPham(Map<String, Object> tk) {
        List<String> p = new ArrayList<>();
        for (Object o : list(tk.get("sai_pham"))) p.add(map(o).get("ten") + " " + py(map(o).get("so")));
        return "Sai phạm " + py(tk.get("sai_pham_tong")) + ": " + String.join(", ", p);
    }

    /** Các dòng (≤ 64 ký tự) in số liệu máy chủ – dùng ở máy chủ dòng lệnh, Khach, MoPhong. */
    static List<String> dongThongKe(Map<String, Object> d) {
        return List.of("Chế độ " + d.get("che_do") + ", trễ xử lý 0–" + g(sod(d.get("tre_ms"))) + " ms",
                "Yêu cầu " + py(d.get("so_yc")) + ": thành công " + py(d.get("thanh_cong")) + ", từ chối " + py(d.get("tu_choi")),
                "Xung đột " + py(d.get("xung_dot")) + ", thử lại " + py(d.get("thu_lai")) + ", hết lượt thử " + py(d.get("het_lan_thu")),
                "Hàng đợi max " + py(d.get("hang_doi_max")) + ", chờ khóa quá hạn " + py(d.get("khoa_cho_lau")),
                "Xử lý µs: p50 " + soUs(d.get("p50_us")) + " p95 " + soUs(d.get("p95_us")) + " p99 " + soUs(d.get("p99_us")),
                "Thông lượng " + py(d.get("thong_luong")) + " yc/s",
                dongSaiPham(d),
                String.valueOf(d.get("tom_tat")));
    }

    static final String TIEU_DE_SO_SANH = "  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms";

    /** hang: danh sách Map {che_do, thanh_cong, sai_pham, xung_dot, yc_s, p99_ms} -> các dòng bảng. */
    static List<String> bangSoSanh(Collection<Map<String, Object>> hang) {
        List<String> ds = new ArrayList<>();
        ds.add(TIEU_DE_SO_SANH);
        for (Map<String, Object> h : hang) {
            Object p = h.get("p99_ms");
            String p99 = p == null ? "–" : f("%.1f", sod(p));
            ds.add(f("  %-12s %10s %8s %8s %5s %7s", h.get("che_do"), py(h.get("thanh_cong")), py(h.get("sai_pham")),
                    py(h.get("xung_dot")), py(h.get("yc_s")), p99));
        }
        return ds;
    }
}
