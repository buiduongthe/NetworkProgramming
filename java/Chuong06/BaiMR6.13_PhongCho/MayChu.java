import java.time.LocalTime;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.PriorityQueue;
import java.util.Set;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.13 – Lõi máy chủ bán vé có phòng chờ ảo: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * Bán 500 vé concert cho rất nhiều người cùng lúc. Ba thao tác của người dùng:
 *     {"loai":"xep_hang","ma_yc":k}         vào phòng chờ -> số thứ tự, số người phía trước, thời gian chờ ước tính
 *     {"loai":"cho_luot","ma_yc":k}         "chờ dài" (long polling): máy chủ CHỈ trả lời khi tới lượt (kèm han_ms –
 *                                           thời gian còn lại để mua) hoặc khi hết vé
 *     {"loai":"mua","ma_yc":k,"so_ve":2}    mua 1–4 vé; khi phòng chờ bật phải đang có lượt còn hạn
 * (kết nối gộp của bộ mô phỏng thêm "nguoi":"u17").
 *
 * Phòng chờ ảo (virtual waiting room): máy chủ cấp lượt theo THÙNG TOKEN (token bucket): token rót vào đều toc_do
 * token/giây, thùng chứa tối đa dung_luong token; mỗi 100 ms luồng nền lấy người đầu hàng (số thứ tự nhỏ nhất) ra, mỗi
 * người tốn một token -> lượt có hạn luot_s giây (mặc định 20 s) để mua. Nhờ vậy phía "mua" (phần đắt: kiểm tra, ghi
 * kho) chỉ nhận đều đặn vài chục người mỗi giây dù hàng chờ có hàng nghìn người. Tắt phòng chờ (phong_cho = 0): ai
 * cũng gọi mua ngay -> dồn cục; máy chủ chỉ cho tối đa gioi_han_xu_ly yêu cầu mua đang xử lý cùng lúc, quá số đó từ
 * chối ngay "qua_tai" (như HTTP 503) -> nhiều từ chối, độ trễ cao.
 *
 * Các ô của KhoTranhChap (đi qua 4 cách xử lý tranh chấp):
 *   "hang" {"cuoi": số thứ tự cuối đã phát}  – xep_hang đọc, +1, ghi: phát số thứ tự;
 *   "kho"  {"con": vé còn, "da_ban": đã bán} – mua đọc, kiểm tra số vé, trừ kho;
 *   "luot:<tên>" {"con": còn lượt?, "han": hạn} – tạo khi cấp lượt; mua đọc cùng "kho" và ghi con = false trong CÙNG
 *                thao tác (tiêu lượt nguyên tử với trừ kho).
 * Sổ kiểm toán đếm ba sai phạm:
 *   ban_qua    "vượt vé"  – số vé bán vượt tổng số vé (theo sổ: cộng số vé mọi đơn được chấp nhận);
 *   khong_luot "sai lượt" – một đơn được chấp nhận khi người đó không còn lượt (lượt đã dùng ở tab khác);
 *   dao_thu_tu "đảo số"   – người vào hàng sau nhận số thứ tự không lớn hơn người vào trước (FIFO bị phá).
 */
final class MayChu {
    private MayChu() {}

    static Map<String, Object> kqHetVe() { return Json.obj("ok", false, "ma_loi", "het_ve", "ly_do", "đã bán hết vé"); }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("ban_qua", "vượt vé");
        m.put("khong_luot", "sai lượt");
        m.put("dao_thu_tu", "đảo số");
        return m;
    }

    static final class SoKiemToanPhongCho extends Chung.SoKiemToan {
        final long tongVe;
        long daBan;                                       // tổng vé theo sổ
        long soMax;                                       // số thứ tự lớn nhất đã phát (theo thứ tự ghi sổ)
        final Set<Object> luot = new HashSet<>();         // người đang có lượt chưa dùng (theo sổ)

        SoKiemToanPhongCho(long tongVe) {
            super(tenSaiPham());
            this.tongVe = tongVe;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            Object loai = gd.get("loai"), ng = gd.get("nguoi");
            if ("xep".equals(loai)) {
                long so = Chung.so(gd.get("so"));
                if (so <= soMax) saiPham("dao_thu_tu", ng + " vào sau nhưng nhận số " + so + " ≤ " + soMax);
                else soMax = so;
            } else if ("luot".equals(loai)) {
                luot.add(ng);
            } else if ("mua".equals(loai)) {
                long soVe = Chung.so(gd.get("so_ve"));
                if (Chung.dung(gd.get("can_luot"))) {
                    if (!luot.remove(ng)) saiPham("khong_luot", ng + " mua " + soVe + " vé khi lượt đã dùng");
                }
                long truoc = daBan;
                daBan += soVe;
                long vuot = daBan - Math.max(truoc, tongVe);
                if (vuot > 0) {                           // đếm theo SỐ VÉ bán vượt
                    dem.merge("ban_qua", vuot - 1, Long::sum);
                    saiPham("ban_qua", ng + " mua " + soVe + " vé, sổ đã bán " + daBan);
                }
            }
        }

        synchronized long daBan() { return daBan; }
    }

    /** Một người trong phòng chờ. */
    static final class Nguoi {
        final long so;
        String trangThai;                                 // cho | luot | da_mua | het_han | het_ve
        Double han;
        List<Consumer<Map<String, Object>>> cho = new ArrayList<>();   // yêu cầu cho_luot đang giữ

        Nguoi(long so, String trangThai) { this.so = so; this.trangThai = trangThai; }
    }

    record ODoi(long so, long stt, String ten) {}         // một chỗ trong hàng (đống ưu tiên theo số thứ tự)

    record LuotCap(double han, String ten) {}

    static Map<String, Object> thamSoMacDinh() {
        return Json.obj("ten_su_kien", "Đêm nhạc Mùa Đông 2026", "gia_ve", 1_500_000L, "tong_ve", 500L, "ve_toi_da", 4L,
                "phong_cho", 1L, "toc_do", 50.0, "dung_luong", 50L, "luot_s", 20.0, "gioi_han_xu_ly", 64L);
    }

    static final class MayChuPhongCho extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        Object khoaPc;                                    // bảo vệ phòng chờ (danh sách người, hàng, thùng token)
        AtomicLong maDon;                                 // mã đơn tăng mãi, kể cả qua các lần đặt lại
        Map<String, Nguoi> nguoi;
        PriorityQueue<ODoi> heap;                         // người đang chờ, số thứ tự nhỏ nhất trước
        long stt;
        ArrayDeque<LuotCap> coLuot;                       // theo thứ tự cấp lượt – để thu hồi lượt quá hạn
        volatile long dangPhucVu, dangCho, daCapLuot, hetHan;
        double token, tToken;
        volatile boolean hetVe;
        volatile long dangXuLy, xuLyMax, quaTai, soDon;
        ArrayDeque<Map<String, Object>> lichSu;

        MayChuPhongCho(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Bán vé MR6.13", List.of("xep_hang", "cho_luot", "mua"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        SoKiemToanPhongCho so() { return (SoKiemToanPhongCho) so; }

        // ------------------------------------------------------------ tạo / đặt lại
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long veToiDa = Chung.so(ts.getOrDefault("ve_toi_da", 1L));
            if (Chung.so(ts.getOrDefault("tong_ve", 1L)) < 1 || veToiDa < 1 || veToiDa > 10 || Chung.so(ts.getOrDefault("gia_ve", 0L)) < 0)
                throw new IllegalArgumentException("tổng vé ≥ 1, vé tối đa 1–10, giá ≥ 0");
            long pc = Chung.so(ts.getOrDefault("phong_cho", 0L));
            if (pc != 0 && pc != 1) throw new IllegalArgumentException("phong_cho = 0 hoặc 1");
            double tocDo = Chung.sod(ts.getOrDefault("toc_do", 1.0));
            if (!(tocDo > 0 && tocDo <= 100_000) || Chung.so(ts.getOrDefault("dung_luong", 1L)) < 1)
                throw new IllegalArgumentException("tốc độ cấp lượt 0–100000 người/s, dung lượng thùng ≥ 1");
            if (Chung.sod(ts.getOrDefault("luot_s", 1.0)) <= 0 || Chung.so(ts.getOrDefault("gioi_han_xu_ly", 1L)) < 1)
                throw new IllegalArgumentException("thời hạn lượt > 0, giới hạn xử lý ≥ 1");
            if (ts.containsKey("ten_su_kien")) {
                String t = String.valueOf(ts.get("ten_su_kien"));
                int n = t.codePointCount(0, t.length());
                if (n < 1 || n > 40) throw new IllegalArgumentException("tên sự kiện 1–40 ký tự");
            }
        }

        @Override
        protected void khoiTao() {
            kho.datO("kho", Json.obj("con", tsL("tong_ve"), "da_ban", 0L));
            kho.datO("hang", Json.obj("cuoi", 0L));
            so = new SoKiemToanPhongCho(tsL("tong_ve"));
            if (khoaPc == null) {
                khoaPc = new Object();
                maDon = new AtomicLong();
                nguoi = new HashMap<>();
            }
            List<Consumer<Map<String, Object>>> cu = new ArrayList<>();
            synchronized (khoaPc) {
                for (Nguoi ng : nguoi.values()) cu.addAll(ng.cho);
                nguoi = new HashMap<>();
                heap = new PriorityQueue<>(Comparator.comparingLong(ODoi::so).thenComparingLong(ODoi::stt));
                stt = 0;
                coLuot = new ArrayDeque<>();
                dangPhucVu = dangCho = daCapLuot = hetHan = 0;
                token = tsL("dung_luong");
                tToken = Chung.bayGio();
                hetVe = false;
                dangXuLy = xuLyMax = quaTai = 0;
                soDon = 0;
                lichSu = new ArrayDeque<>();
            }
            for (Consumer<Map<String, Object>> cb : cu)       // ai đang "chờ dài" từ phiên trước: trả lời ngay
                cb.accept(Json.obj("ok", false, "ma_loi", "dat_lai", "ly_do", "phiên bán vé được đặt lại"));
        }

        /** Đổi tốc độ cấp lượt ngay khi đang chạy (không đặt lại phiên). */
        void datTocDo(double tocDo) {
            kiemThamSo(Json.obj("toc_do", tocDo));
            thamSo.put("toc_do", tocDo);
            thongBao("Tốc độ cấp lượt: " + Chung.g(tocDo) + " người/s");
            baoDoi();
        }

        // ------------------------------------------------------------ yêu cầu
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String ng, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            Object loai = tin.get("loai");
            if ("mua".equals(loai)) mua(ng, tin, traLoi);
            else if (tsL("phong_cho") == 0)
                traLoi.accept(Json.obj("ok", false, "ma_loi", "pc_tat", "ly_do", "phòng chờ đang tắt – mua trực tiếp"));
            else if ("xep_hang".equals(loai)) xepHang(ng, traLoi);
            else choLuot(ng, traLoi);
        }

        /** Số người phía trước và thời gian chờ ước tính (gọi dưới khoaPc). */
        private Map<String, Object> viTri(Nguoi n) {
            long truoc = n.trangThai.equals("cho") ? Math.max(0, n.so - dangPhucVu - 1) : 0;
            return Json.obj("so", n.so, "truoc", truoc, "cho_s", Math.round((truoc + 1) / tsD("toc_do") * 10) / 10.0,
                    "trang_thai", n.trangThai);
        }

        private void xepHang(String ten, Consumer<Map<String, Object>> traLoi) {
            Map<String, Object> kq = null;
            synchronized (khoaPc) {
                if (hetVe) kq = kqHetVe();
                else if (nguoi.containsKey(ten)) {                   // đã trong hàng (tải lại trang, tab thứ hai): giữ chỗ cũ
                    kq = Json.obj("ok", true, "da_co", true);
                    kq.putAll(viTri(nguoi.get(ten)));
                }
            }
            if (kq != null) {
                traLoi.accept(kq);
                return;
            }
            Chung.XuLy xuLy = doc -> {                                 // phát số thứ tự: đọc – cộng 1 – ghi
                long s = Chung.so(Chung.map(doc.get("hang")).get("cuoi")) + 1;
                return new Chung.Kq(Json.obj("ok", true, "so", s), Json.obj("hang", Json.obj("cuoi", s)));
            };
            Consumer<Map<String, Object>> khiGhi = k -> {              // trong vùng găng: đưa vào hàng, ghi sổ
                boolean moi;
                long s = Chung.so(k.get("so"));
                synchronized (khoaPc) {
                    Nguoi n = nguoi.get(ten);
                    moi = n == null;
                    if (moi && hetVe) {                               // vừa hết vé lúc xếp hàng: cho_luot báo hết vé ngay
                        n = new Nguoi(s, "het_ve");
                        nguoi.put(ten, n);
                    } else if (moi) {
                        n = new Nguoi(s, "cho");
                        nguoi.put(ten, n);
                        heap.add(new ODoi(s, stt++, ten));
                        dangCho++;
                    } else k.put("da_co", true);
                    k.putAll(viTri(n));
                }
                if (moi) so.ghi(Json.obj("loai", "xep", "nguoi", ten, "so", s));
                baoDoi();
            };
            kho.thucHien(List.of("hang"), xuLy, traLoi, khiGhi);
        }

        /** Chờ dài: chưa tới lượt thì GIỮ traLoi, luồng nền gọi khi cấp lượt / hết vé. */
        private void choLuot(String ten, Consumer<Map<String, Object>> traLoi) {
            Map<String, Object> kq;
            synchronized (khoaPc) {
                Nguoi n = nguoi.get(ten);
                if (n == null) kq = Json.obj("ok", false, "ma_loi", "chua_xep_hang", "ly_do", "chưa vào hàng chờ");
                else if (n.trangThai.equals("cho")) {
                    n.cho.add(traLoi);
                    return;
                } else if (n.trangThai.equals("luot"))
                    kq = Json.obj("ok", true, "luot", true, "so", n.so, "han_ms", Math.max(0, Math.round((n.han - Chung.bayGio()) * 1000)));
                else if (n.trangThai.equals("da_mua")) kq = Json.obj("ok", false, "ma_loi", "da_mua", "ly_do", "lượt này đã dùng để mua");
                else if (n.trangThai.equals("het_han")) kq = Json.obj("ok", false, "ma_loi", "het_luot", "ly_do", "lượt mua đã hết hạn");
                else kq = kqHetVe();
            }
            traLoi.accept(kq);
        }

        private void mua(String ten, Map<String, Object> tin, Consumer<Map<String, Object>> traLoi) {
            Object x = tin.get("so_ve");
            long toiDa = tsL("ve_toi_da");
            if (!Chung.laSoNguyen(x) || Chung.so(x) < 1 || Chung.so(x) > toiDa) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "số vé 1–" + toiDa));
                return;
            }
            long soVe = Chung.so(x);
            boolean qua;
            synchronized (khoaPc) {                                    // giới hạn số yêu cầu mua đang xử lý cùng lúc
                qua = dangXuLy >= tsL("gioi_han_xu_ly");
                if (qua) quaTai++;
                else {
                    dangXuLy++;
                    xuLyMax = Math.max(xuLyMax, dangXuLy);
                }
            }
            if (qua) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "qua_tai", "ly_do", "hệ thống quá tải, vui lòng thử lại"));
                return;
            }
            boolean pc = tsL("phong_cho") != 0;
            long gia = tsL("gia_ve");
            String oLuot = "luot:" + ten;                              // ô lượt của người này (tạo khi được cấp lượt)
            Chung.XuLy xuLy = doc -> {                                 // kiểm tra lượt + số vé còn; trừ kho VÀ tiêu lượt
                Map<String, Object> k = Chung.map(doc.get("kho"));
                if (pc) {
                    Map<String, Object> l = Chung.map(doc.get(oLuot));
                    if (!Chung.dung(l.get("con")))
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_mua", "ly_do", "lượt này đã dùng để mua"), null);
                    if (Chung.bayGio() > Chung.sod(l.get("han")))
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_luot", "ly_do", "lượt mua đã hết hạn"), null);
                }
                long con = Chung.so(k.get("con"));
                if (con <= 0) return new Chung.Kq(Chung.sua(kqHetVe(), "con", 0L), null);
                if (con < soVe)
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_ve", "ly_do", "chỉ còn " + con + " vé", "con", con), null);
                Map<String, Object> ghi = Json.obj("kho", Json.obj("con", con - soVe, "da_ban", Chung.so(k.get("da_ban")) + soVe));
                if (pc) ghi.put(oLuot, Chung.sua(Chung.map(doc.get(oLuot)), "con", false));
                return new Chung.Kq(Json.obj("ok", true, "so_ve", soVe, "thanh_tien", soVe * gia, "con", con - soVe), ghi);
            };
            Consumer<Map<String, Object>> khiGhi = kq -> {             // trong vùng găng: tiêu lượt, ghi sổ
                kq.put("ma_don", maDon.incrementAndGet());
                if (pc) {
                    synchronized (khoaPc) {
                        Nguoi n = nguoi.get(ten);
                        if (n != null) n.trangThai = "da_mua";
                    }
                }
                so.ghi(Json.obj("loai", "mua", "nguoi", ten, "so_ve", soVe, "can_luot", pc));
                synchronized (khoaPc) {
                    soDon++;
                    lichSu.addFirst(Json.obj("ma", kq.get("ma_don"), "gio", LocalTime.now().format(GiaoDienChung.GIO),
                            "nguoi", ten, "so_ve", soVe));
                    if (lichSu.size() > 30) lichSu.removeLast();
                }
                baoDoi();
            };
            Consumer<Map<String, Object>> tra = kq -> {
                synchronized (khoaPc) { dangXuLy--; }
                if ("khong_co".equals(kq.get("ma_loi")))              // chưa có ô lượt: chưa được cấp lượt
                    kq = Json.obj("ok", false, "ma_loi", "khong_luot", "ly_do", "chưa tới lượt – hãy vào hàng chờ");
                traLoi.accept(kq);
            };
            kho.thucHien(pc ? List.of("kho", oLuot) : List.of("kho"), xuLy, tra, khiGhi);
        }

        // ------------------------------------------------------------ luồng nền (100 ms): thùng token
        @Override
        protected void dinhKy(double bayGio) {
            Map<String, Object> k = doc("kho");
            if (k == null) return;                                     // đang đặt lại
            List<Object[]> tha = new ArrayList<>();
            boolean baoHet = false;
            synchronized (khoaPc) {
                while (!coLuot.isEmpty() && coLuot.peekFirst().han() <= bayGio) {   // thu hồi lượt quá hạn
                    LuotCap l = coLuot.pollFirst();
                    Nguoi n = nguoi.get(l.ten());
                    if (n != null && n.trangThai.equals("luot") && n.han == l.han()) {
                        n.trangThai = "het_han";
                        hetHan++;
                    }
                }
                if (Chung.so(k.get("con")) <= 0 && !hetVe) {             // hết vé: báo mọi người đang chờ, đóng hàng
                    hetVe = baoHet = true;
                    for (Nguoi n : nguoi.values())
                        if (n.trangThai.equals("cho")) {
                            n.trangThai = "het_ve";
                            for (Consumer<Map<String, Object>> cb : n.cho) tha.add(new Object[] {cb, kqHetVe()});
                            n.cho = new ArrayList<>();
                        }
                    heap.clear();
                    dangCho = 0;
                } else if (tsL("phong_cho") != 0 && !hetVe) {
                    token = Math.min(tsL("dung_luong"), token + tsD("toc_do") * (bayGio - tToken));
                    while (!heap.isEmpty() && token >= 1) {
                        ODoi d = heap.poll();
                        Nguoi n = nguoi.get(d.ten());
                        if (n == null || !n.trangThai.equals("cho") || n.so != d.so()) continue;
                        token -= 1;
                        dangCho--;
                        so.ghi(Json.obj("loai", "luot", "nguoi", d.ten()));       // sổ biết lượt TRƯỚC khi người đó thấy lượt
                        n.trangThai = "luot";
                        n.han = bayGio + tsD("luot_s");
                        kho.datO("luot:" + d.ten(), Json.obj("con", true, "han", n.han));   // ô mới: chưa ai chạm tới
                        coLuot.addLast(new LuotCap(n.han, d.ten()));
                        dangPhucVu = Math.max(dangPhucVu, d.so());
                        daCapLuot++;
                        Map<String, Object> kq = Json.obj("ok", true, "luot", true, "so", d.so(), "han_ms", Math.round(tsD("luot_s") * 1000));
                        for (Consumer<Map<String, Object>> cb : n.cho) tha.add(new Object[] {cb, new LinkedHashMap<>(kq)});
                        n.cho = new ArrayList<>();
                    }
                }
                tToken = bayGio;
            }
            if (baoHet) thongBao("Đã bán hết " + tsL("tong_ve") + " vé");
            for (Object[] t : tha) {
                @SuppressWarnings("unchecked")
                Consumer<Map<String, Object>> cb = (Consumer<Map<String, Object>>) t[0];
                cb.accept(Chung.map(t[1]));
            }
            if (!tha.isEmpty()) baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        Map<String, Object> doc(String o) {
            try {
                return kho.doc(o);
            } catch (NullPointerException e) {                         // đang đặt lại
                return null;
            }
        }

        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            Map<String, Object> k = doc("kho"), h = doc("hang");
            if (k == null) k = Json.obj("con", 0L, "da_ban", 0L);
            List<Object> ls;
            long tk;
            synchronized (khoaPc) {
                ls = new ArrayList<>(lichSu);
                tk = (long) token;
            }
            return Json.obj("su_kien", Json.obj("ten", thamSo.get("ten_su_kien"), "gia_ve", thamSo.get("gia_ve"),
                            "tong_ve", thamSo.get("tong_ve"), "ve_toi_da", thamSo.get("ve_toi_da"), "luot_s", thamSo.get("luot_s")),
                    "phong_cho", tsL("phong_cho") != 0, "con_lai", k.get("con"), "da_ban", k.get("da_ban"),
                    "toc_do", thamSo.get("toc_do"), "dung_luong", thamSo.get("dung_luong"), "token", tk,
                    "dang_phuc_vu", dangPhucVu, "so_cap", h == null ? 0L : h.get("cuoi"),
                    "dang_cho", dangCho, "da_cap_luot", daCapLuot, "het_han", hetHan,
                    "het_ve", hetVe, "dang_xu_ly", dangXuLy, "xu_ly_max", xuLyMax,
                    "gioi_han", thamSo.get("gioi_han_xu_ly"), "qua_tai", quaTai, "so_don", soDon,
                    "ban_so", so().daBan(), "lich_su", new ArrayList<>(ls.subList(0, Math.min(10, ls.size()))));
        }

        @Override
        protected String tomTat() {
            Map<String, Object> k = doc("kho");
            return "Sổ: bán " + so().daBan() + "/" + tsL("tong_ve") + " vé, " + soDon + " đơn; kho còn " + (k == null ? 0 : k.get("con"));
        }
    }
}
