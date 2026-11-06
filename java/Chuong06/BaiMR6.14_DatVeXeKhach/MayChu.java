import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.14 – Lõi máy chủ đặt vé xe khách theo chặng: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối) và MR6.8.
 * Chuyến xe qua các trạm Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang (3 chặng). Hai loại xe:
 *     giuong – xe giường nằm 2 tầng, 40 giường: tầng dưới D01–D20, tầng trên T01–T20 (3 dãy × 7 hàng, hàng cuối 2 giường);
 *     ghe    – xe ghế ngồi 50 chỗ 01–50 (11 hàng × 4 ghế, lối đi ở giữa, hàng cuối 6 ghế liền).
 * Khách đặt một ĐOẠN [tu, den) (chỉ số trạm, tu < den), nên cùng một giường bán được cho nhiều người nếu các đoạn
 * không chồng nhau. Ba thao tác ghi:
 *     {"loai":"giu_cho","ma_yc":k,"ghe":["D01"],"tu":0,"den":2[,"nguoi":"u17"]}   giữ chỗ có thời hạn (TTL)
 *     {"loai":"thanh_toan","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                 chỗ mình đang giữ -> vé
 *     {"loai":"nha","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                        trả lại chỗ đang giữ
 * Mỗi cặp (ghế, chặng) là MỘT ô của KhoTranhChap, tên ô "D01#1" (chặng 1 = Bảo Lộc → Đà Lạt):
 *     {"tt":"trong"|"giu"|"ban","nguoi","het_han"(đồng hồ đơn điệu),"ma_ve","nguoi_het_han"}
 * Giữ 2 giường cho đoạn 3 chặng = khóa 6 ô theo thứ tự tên ô (không khóa chết). Luật như MR6.8: giữ được khi MỌI ô đều
 * trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); hết hạn sau ttl_s giây – luồng nền dọn ô quá hạn mỗi
 * 100 ms; thanh toán chỉ nhận ô mình giữ, còn hạn.
 *
 * Sổ kiểm toán đếm hai sai phạm:
 *   trung_cho – một (ghế, chặng) được giữ/bán cho người thứ hai khi người khác đang giữ (còn hạn) hoặc đã mua;
 *   tt_sai    – thanh toán được chấp nhận cho chỗ không do mình giữ hoặc lượt giữ đã hết hạn.
 */
final class MayChu {
    private MayChu() {}

    static final List<String> TRAM = List.of("Sài Gòn", "Bảo Lộc", "Đà Lạt", "Nha Trang");
    static final List<String> MA_TRAM = List.of("SG", "BL", "DL", "NT");
    static final int SO_CHANG = TRAM.size() - 1;
    static final Map<String, List<Long>> GIA_CHANG = Map.of(               // giá minh họa mỗi chặng, đồng/chỗ
            "giuong", List.of(220_000L, 130_000L, 170_000L), "ghe", List.of(170_000L, 100_000L, 130_000L));
    static final Map<String, String> TEN_XE = Map.of("giuong", "Xe giường nằm 40 chỗ", "ghe", "Xe ghế ngồi 50 chỗ");
    static final int TOI_DA_GHE = 6;                    // một yêu cầu tối đa 6 chỗ

    /** Một chỗ trên sơ đồ xe. */
    record ChoXe(String ma, int tang, int hang, int cot) {}

    /** Sơ đồ xe theo thứ tự hiển thị. giuong: tầng 0 = dưới (D), 1 = trên (T); cột 0, 1, 2; hàng 6 chỉ có cột 0 và 2.
     *  ghe: tầng 0; 6 cột: hàng 0–10 dùng cột 0, 1 | lối đi | 4, 5; hàng cuối (11) đủ 6 cột. */
    static List<ChoXe> boTri(String loaiXe) {
        List<ChoXe> ds = new ArrayList<>();
        if (loaiXe.equals("giuong")) {
            for (int tang = 0; tang < 2; tang++) {
                int n = 0;
                for (int hang = 0; hang < 7; hang++)
                    for (int cot : hang < 6 ? new int[]{0, 1, 2} : new int[]{0, 2})
                        ds.add(new ChoXe((tang == 0 ? "D" : "T") + String.format("%02d", ++n), tang, hang, cot));
            }
        } else {
            int n = 0;
            for (int hang = 0; hang < 12; hang++)
                for (int cot : hang < 11 ? new int[]{0, 1, 4, 5} : new int[]{0, 1, 2, 3, 4, 5})
                    ds.add(new ChoXe(String.format("%02d", ++n), 0, hang, cot));
        }
        return ds;
    }

    static List<String> dsMaGhe(String loaiXe) {
        List<String> ds = new ArrayList<>();
        for (ChoXe x : boTri(loaiXe)) ds.add(x.ma());
        return ds;
    }

    static String tenChang(int k) { return TRAM.get(k) + "–" + TRAM.get(k + 1); }

    static String tenDoan(int tu, int den) { return TRAM.get(tu) + " → " + TRAM.get(den); }

    static String tenO(String o) {
        String[] p = o.split("#");
        return p[0] + " (" + tenChang(Integer.parseInt(p[1])) + ")";
    }

    static List<String> oCua(List<String> ghe, int tu, int den) {
        List<String> ds = new ArrayList<>();
        for (String g : ghe) for (int k = tu; k < den; k++) ds.add(g + "#" + k);
        return ds;
    }

    static Map<String, Object> trong(Object lanCuoi) {
        return Json.obj("tt", "trong", "nguoi", null, "het_han", null, "ma_ve", null, "nguoi_het_han", lanCuoi);
    }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("trung_cho", "trùng chỗ");
        m.put("tt_sai", "thanh toán sai");
        return m;
    }

    static final class SoKiemToanDatVeXe extends Chung.SoKiemToan {
        final Map<String, Map<String, Object>> o = new LinkedHashMap<>();   // theo sổ: ô -> {tt, nguoi, het_han}
        long soVe;                                                        // số vé (ghế × đoạn) đã bán theo sổ

        SoKiemToanDatVeXe() { super(tenSaiPham()); }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            Object nguoi = gd.get("nguoi");
            double luc = Chung.sod(gd.get("luc"));
            String loai = (String) gd.get("loai");
            if (loai.equals("ban")) soVe += Chung.so(gd.get("so_ve"));
            for (Object x : Chung.list(gd.get("o"))) {
                String oo = (String) x;
                Map<String, Object> cu = o.get(oo);
                if (loai.equals("giu")) {
                    if (cu != null && "ban".equals(cu.get("tt"))) {
                        saiPham("trung_cho", tenO(oo) + " giữ cho " + nguoi + " khi đã bán cho " + cu.get("nguoi"));
                        continue;
                    }
                    if (cu != null && !cu.get("nguoi").equals(nguoi) && luc < Chung.sod(cu.get("het_han")))
                        saiPham("trung_cho", tenO(oo) + " giữ cho " + nguoi + " khi " + cu.get("nguoi") + " đang giữ");
                    o.put(oo, Json.obj("tt", "giu", "nguoi", nguoi, "het_han", gd.get("het_han")));
                } else if (loai.equals("ban")) {
                    if (cu != null && "ban".equals(cu.get("tt"))) {
                        saiPham("trung_cho", tenO(oo) + " bán cho " + nguoi + " khi đã bán cho " + cu.get("nguoi"));
                        continue;
                    }
                    if (cu == null || !cu.get("nguoi").equals(nguoi))
                        saiPham("tt_sai", tenO(oo) + ": " + nguoi + " trả tiền chỗ không do mình giữ");
                    else if (luc >= Chung.sod(cu.get("het_han")))
                        saiPham("tt_sai", tenO(oo) + ": " + nguoi + " trả tiền khi đã hết hạn giữ");
                    o.put(oo, Json.obj("tt", "ban", "nguoi", nguoi, "het_han", null));
                } else if (cu != null && "giu".equals(cu.get("tt")) && cu.get("nguoi").equals(nguoi)) {   // nhả / hết hạn
                    o.remove(oo);
                }
            }
        }
    }

    static Map<String, Object> thamSoMacDinh() {
        return Json.obj("ttl_s", 30.0, "loai_xe", "giuong", "ghe_nong", "D01", "chuyen", "07:30", "nha_xe", "Xe khách Cao Nguyên");
    }

    static final class MayChuDatVeXe extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        volatile List<ChoXe> boTri;
        volatile List<String> dsGhe;
        volatile Set<String> gheHopLe;
        volatile List<String> tatCaO;
        Set<String> dangDon;                       // ô quá hạn đang được luồng nền dọn
        Set<String> nguoiThat;                     // tên người dùng thật (in nhật ký cho họ)
        AtomicLong demGhi;                         // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
        volatile long phien;

        MayChuDatVeXe(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Đặt vé xe khách MR6.14", List.of("giu_cho", "thanh_toan", "nha"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        // ------------------------------------------------------------ tạo / đặt lại
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            double ttl = Chung.sod(ts.getOrDefault("ttl_s", 30.0));
            if (!(ttl >= 1 && ttl <= 600)) throw new IllegalArgumentException("thời hạn giữ chỗ 1–600 s");
            String loai = String.valueOf(ts.getOrDefault("loai_xe", thamSo.get("loai_xe")));
            if (!GIA_CHANG.containsKey(loai)) throw new IllegalArgumentException("loại xe: giuong hoặc ghe");
            Object nong = ts.getOrDefault("ghe_nong", "");
            if (!"".equals(nong) && !dsMaGhe(loai).contains(nong))
                throw new IllegalArgumentException("ghế nóng phải là mã chỗ của " + TEN_XE.get(loai).toLowerCase() + " hoặc rỗng");
            if (ts.containsKey("nha_xe")) {
                String p = String.valueOf(ts.get("nha_xe"));
                int n = p.codePointCount(0, p.length());
                if (n < 1 || n > 40) throw new IllegalArgumentException("tên nhà xe 1–40 ký tự");
            }
        }

        /** Ghế nóng đang dùng: tham số ghe_nong nếu là chỗ của loại xe hiện tại, không thì chỗ đầu (D01 / 01) – đổi loại
         *  xe mà không đặt ghe_nong thì ghế nóng tự về chỗ đầu; rỗng = không có ghế nóng. */
        String gheNong() {
            String g = tsS("ghe_nong");
            return g.isEmpty() || gheHopLe.contains(g) ? g : dsGhe.get(0);
        }

        @Override
        protected void khoiTao() {
            List<ChoXe> bt = MayChu.boTri(tsS("loai_xe"));
            List<String> ds = new ArrayList<>(), tatCa = new ArrayList<>();
            for (ChoXe x : bt) {
                ds.add(x.ma());
                for (int k = 0; k < SO_CHANG; k++) tatCa.add(x.ma() + "#" + k);
            }
            for (String o : tatCa) kho.datO(o, trong(null));
            boTri = bt;
            dsGhe = ds;
            gheHopLe = new HashSet<>(ds);
            tatCaO = tatCa;
            so = new SoKiemToanDatVeXe();
            dangDon = ConcurrentHashMap.newKeySet();
            nguoiThat = ConcurrentHashMap.newKeySet();
            if (demGhi == null) {
                demGhi = new AtomicLong();
                phien = 0;
            }
        }

        // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
        /** Danh sách chỗ hợp lệ (1–6 mã của loại xe hiện tại, không trùng) hoặc null. */
        private List<String> docGhe(Object x) {
            if (!(x instanceof List<?> l) || l.isEmpty() || l.size() > TOI_DA_GHE) return null;
            Set<String> khac = new HashSet<>();
            List<String> kq = new ArrayList<>();
            for (Object g : l) {
                khac.add(Chung.py(g));
                if (!(g instanceof String s) || !gheHopLe.contains(s)) return null;
                kq.add(s);
            }
            return khac.size() == l.size() ? kq : null;
        }

        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            List<String> ghe = docGhe(tin.get("ghe"));
            if (ghe == null) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "ghế: danh sách 1–" + TOI_DA_GHE
                        + " mã chỗ của " + TEN_XE.get(tsS("loai_xe")).toLowerCase()));
                return;
            }
            if (!(tin.get("tu") instanceof Long a) || !(tin.get("den") instanceof Long b) || a < 0 || a >= b || b > SO_CHANG) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "đoạn đường: 0 ≤ tu < den ≤ " + SO_CHANG
                        + " (chỉ số trạm)"));
                return;
            }
            int tu = (int) (long) (Long) tin.get("tu"), den = (int) (long) (Long) tin.get("den");
            List<String> oYC = oCua(ghe, tu, den);
            boolean that = khach.soNguoi == null;
            if (that) nguoiThat.add(nguoi);
            double ttl = tsD("ttl_s");
            String chuyen = tsS("chuyen");
            long giaMot = 0;
            for (long x : GIA_CHANG.get(tsS("loai_xe")).subList(tu, den)) giaMot += x;
            long gia = giaMot * ghe.size();
            Consumer<Map<String, Object>> tra = kq -> {                 // bỏ các trường nội bộ ("_…") trước khi gửi
                Map<String, Object> m = new LinkedHashMap<>();
                kq.forEach((k, v) -> { if (!k.startsWith("_")) m.put(k, v); });
                traLoi.accept(m);
            };
            Object loai = tin.get("loai");
            if ("giu_cho".equals(loai)) {
                Chung.XuLy xuLy = doc -> {                              // hàm THUẦN: mọi ô phải trống / quá hạn / của mình
                    double luc = Chung.bayGio(), conLai = ttl;
                    List<Object> moi = new ArrayList<>();
                    for (String o : oYC) {
                        Map<String, Object> v = Chung.map(doc.get(o));
                        String g = o.split("#")[0];
                        int k = Integer.parseInt(o.split("#")[1]);
                        if ("ban".equals(v.get("tt")))
                            return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_ban", "ly_do", "chỗ " + g + " chặng " + tenChang(k)
                                    + " đã bán", "ghe_ban", new ArrayList<>(List.of(g)), "tu", (long) tu, "den", (long) den), null);
                        if ("giu".equals(v.get("tt")) && luc < Chung.sod(v.get("het_han"))) {
                            if (!nguoi.equals(v.get("nguoi")))
                                return new Chung.Kq(Json.obj("ok", false, "ma_loi", "dang_giu", "ly_do", "chỗ " + g + " chặng "
                                        + tenChang(k) + " đang có người giữ", "ghe_ban", new ArrayList<>(List.of(g)),
                                        "tu", (long) tu, "den", (long) den), null);
                            conLai = Math.min(conLai, Chung.sod(v.get("het_han")) - luc);
                        } else moi.add(o);
                    }
                    double hetHan = luc + ttl;
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (Object o : moi)
                        ghi.put((String) o, Json.obj("tt", "giu", "nguoi", nguoi, "het_han", hetHan, "ma_ve", null, "nguoi_het_han", null));
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(ghe), "tu", (long) tu, "den", (long) den,
                            "nguoi", nguoi, "het_han_ms", Math.round(conLai * 1000), "tien", gia, "_moi", moi, "_luc", luc,
                            "_het_han", hetHan), ghi.isEmpty() ? null : ghi);
                };
                kho.thucHien(oYC, xuLy, tra, kq -> daGiu(kq, that));
            } else if ("thanh_toan".equals(loai)) {
                Chung.XuLy xuLy = doc -> {
                    double luc = Chung.bayGio();
                    String hetHan = null, khongGiu = null;               // ưu tiên báo "hết hạn" nếu có dấu vết lượt giữ của mình
                    for (String o : oYC) {
                        Map<String, Object> v = Chung.map(doc.get(o));
                        String g = o.split("#")[0];
                        boolean cuaToi = "giu".equals(v.get("tt")) && nguoi.equals(v.get("nguoi"));
                        if (cuaToi && luc < Chung.sod(v.get("het_han"))) continue;
                        if (cuaToi || ("trong".equals(v.get("tt")) && nguoi.equals(v.get("nguoi_het_han")))) {
                            if (hetHan == null) hetHan = g;
                        } else if (khongGiu == null) khongGiu = g;
                    }
                    if (hetHan != null)
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_han", "ly_do", "giữ chỗ " + hetHan + " đã hết hạn"), null);
                    if (khongGiu != null)
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_giu", "ly_do", "chỗ " + khongGiu + " không do bạn giữ"), null);
                    String ma = "XK" + chuyen.replace(":", "");
                    List<Object> ve = new ArrayList<>();
                    Map<String, String> maVe = new LinkedHashMap<>();
                    for (String g : ghe) {
                        String mv = ma + "-" + g + "-" + MA_TRAM.get(tu) + MA_TRAM.get(den);
                        ve.add(mv);
                        maVe.put(g, mv);
                    }
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (String o : oYC)
                        ghi.put(o, Json.obj("tt", "ban", "nguoi", nguoi, "het_han", null, "ma_ve", maVe.get(o.split("#")[0]),
                                "nguoi_het_han", null));
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(ghe), "tu", (long) tu, "den", (long) den,
                            "nguoi", nguoi, "ma_ve", ve, "tien", gia, "_luc", luc), ghi);
                };
                kho.thucHien(oYC, xuLy, tra, kq -> daBan(kq, that));
            } else {
                Chung.XuLy xuLy = doc -> {
                    List<Object> nha = new ArrayList<>();
                    for (String o : oYC) {
                        Map<String, Object> v = Chung.map(doc.get(o));
                        if ("giu".equals(v.get("tt")) && nguoi.equals(v.get("nguoi"))) nha.add(o);
                    }
                    if (nha.isEmpty())
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_giu", "ly_do", "bạn không giữ chỗ nào trong số này"), null);
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (Object o : nha) ghi.put((String) o, trong(null));
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(ghe), "tu", (long) tu, "den", (long) den,
                            "nguoi", nguoi, "_o", nha), ghi);
                };
                kho.thucHien(oYC, xuLy, tra, kq -> daNha(kq, that));
            }
        }

        // khiGhi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự lần
        // ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
        private void danhSo(Map<String, Object> kq) {
            long p = demGhi.incrementAndGet();
            phien = p;
            kq.put("phien", p);
        }

        static String noi(Object ds) {
            List<String> p = new ArrayList<>();
            for (Object o : Chung.list(ds)) p.add(String.valueOf(o));
            return String.join(", ", p);
        }

        private static String doan(Map<String, Object> kq) {
            return tenDoan((int) Chung.so(kq.get("tu")), (int) Chung.so(kq.get("den")));
        }

        private void daGiu(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "giu", "o", kq.get("_moi"), "nguoi", kq.get("nguoi"), "luc", kq.get("_luc"), "het_han", kq.get("_het_han")));
            if (that) thongBao(kq.get("nguoi") + " giữ " + noi(kq.get("ghe")) + " (" + doan(kq) + ", " + Chung.g(tsD("ttl_s")) + " s)");
            baoDoi();
        }

        private void daBan(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            List<String> ghe = new ArrayList<>();
            for (Object g : Chung.list(kq.get("ghe"))) ghe.add((String) g);
            List<Object> o = new ArrayList<>(oCua(ghe, (int) Chung.so(kq.get("tu")), (int) Chung.so(kq.get("den"))));
            so.ghi(Json.obj("loai", "ban", "o", o, "nguoi", kq.get("nguoi"), "luc", kq.get("_luc"), "so_ve", (long) ghe.size()));
            if (that) thongBao(kq.get("nguoi") + " mua " + ghe.size() + " vé " + noi(kq.get("ghe")) + " " + doan(kq)
                    + " (" + Chung.tien(Chung.so(kq.get("tien"))) + ")");
            baoDoi();
        }

        private void daNha(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "nha", "o", kq.get("_o"), "nguoi", kq.get("nguoi")));
            if (that) thongBao(kq.get("nguoi") + " nhả " + noi(kq.get("ghe")) + " (" + doan(kq) + ")");
            baoDoi();
        }

        // ------------------------------------------------------------ dọn ô quá hạn (luồng nền, 100 ms)
        @Override
        protected void dinhKy(double bayGio) {
            for (String o : tatCaO) {
                Map<String, Object> v = kho.doc(o);
                if (v == null || !"giu".equals(v.get("tt")) || bayGio < Chung.sod(v.get("het_han")) || dangDon.contains(o)) continue;
                dangDon.add(o);
                Chung.XuLy xuLy = doc -> {                             // nhả ô quá hạn cũng là một thao tác ghi
                    Map<String, Object> x = Chung.map(doc.get(o));
                    if (!"giu".equals(x.get("tt")) || Chung.bayGio() < Chung.sod(x.get("het_han")))
                        return new Chung.Kq(Json.obj("ok", false), null);     // vừa được thanh toán / giữ lại
                    return new Chung.Kq(Json.obj("ok", true, "o", new ArrayList<Object>(List.of(o)), "nguoi", x.get("nguoi")),
                            Json.obj(o, trong(x.get("nguoi"))));
                };
                kho.thucHien(List.of(o), xuLy, kq -> dangDon.remove(o), this::daHetHan);
            }
        }

        private void daHetHan(Map<String, Object> kq) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "het_han", "o", kq.get("o"), "nguoi", kq.get("nguoi")));
            if (nguoiThat.contains(String.valueOf(kq.get("nguoi"))))
                thongBao("Hết hạn giữ " + tenO((String) Chung.list(kq.get("o")).get(0)) + " (" + kq.get("nguoi") + "), chỗ trống lại");
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        /** so_do[i] = chuỗi SO_CHANG ký tự cho chỗ ghe[i]: '.' trống, 'g' đang giữ, 'b' đã bán (từng chặng);
         *  chu / con_lai_ms theo tên ô "D01#1". */
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            long ph = phien;                                     // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
            double bayGio = Chung.bayGio();
            List<Object> soDo = new ArrayList<>();
            Map<String, Object> chu = new LinkedHashMap<>(), conLai = new LinkedHashMap<>();
            long dTrong = 0, dGiu = 0, dBan = 0;
            List<ChoXe> bt = boTri;
            for (ChoXe x : bt) {
                StringBuilder dong = new StringBuilder();
                for (int k = 0; k < SO_CHANG; k++) {
                    String o = x.ma() + "#" + k;
                    Map<String, Object> v = kho.doc(o);
                    if (v != null && "giu".equals(v.get("tt")) && bayGio < Chung.sod(v.get("het_han"))) {
                        dong.append('g');
                        chu.put(o, v.get("nguoi"));
                        conLai.put(o, Math.round((Chung.sod(v.get("het_han")) - bayGio) * 1000));
                        dGiu++;
                    } else if (v != null && "ban".equals(v.get("tt"))) {
                        dong.append('b');
                        chu.put(o, v.get("nguoi"));
                        dBan++;
                    } else {
                        dong.append('.');
                        dTrong++;
                    }
                }
                soDo.add(dong.toString());
            }
            List<Object> ghe = new ArrayList<>(), boTriJ = new ArrayList<>();
            for (ChoXe x : bt) {
                ghe.add(x.ma());
                boTriJ.add(new ArrayList<Object>(List.of((long) x.tang(), (long) x.hang(), (long) x.cot())));
            }
            String loai = tsS("loai_xe");
            return Json.obj("phien", ph, "nha_xe", thamSo.get("nha_xe"), "chuyen", thamSo.get("chuyen"), "loai_xe", loai,
                    "ten_xe", TEN_XE.get(loai), "tram", new ArrayList<Object>(TRAM), "gia_chang", new ArrayList<Object>(GIA_CHANG.get(loai)),
                    "ttl_s", thamSo.get("ttl_s"), "ghe_nong", gheNong(), "ghe", ghe, "bo_tri", boTriJ, "so_do", soDo, "chu", chu,
                    "con_lai_ms", conLai, "dem", Json.obj("trong", dTrong, "giu", dGiu, "ban", dBan));
        }

        @Override
        protected String tomTat() {
            long ban = 0, giu = 0;
            double bayGio = Chung.bayGio();
            List<String> tatCa = tatCaO;
            for (String o : tatCa) {
                Map<String, Object> v = kho.doc(o);
                if (v == null) continue;
                if ("ban".equals(v.get("tt"))) ban++;
                if ("giu".equals(v.get("tt")) && bayGio < Chung.sod(v.get("het_han"))) giu++;
            }
            return "Bán " + ban + "/" + tatCa.size() + " chỗ-chặng, giữ " + giu + ", sổ " + ((SoKiemToanDatVeXe) so).soVe + " vé";
        }
    }
}
