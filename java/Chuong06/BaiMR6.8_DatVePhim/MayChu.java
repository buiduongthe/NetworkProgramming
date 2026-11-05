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
 * Bài mở rộng MR6.8 – Lõi máy chủ đặt vé xem phim: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * Phòng chiếu 8 hàng (A–H) × 12 ghế; n người cùng chọn ghế. Ba thao tác ghi:
 *     {"loai":"giu_cho","ma_yc":k,"ghe":["E7","E8"][,"nguoi":"u17"]}    giữ chỗ có thời hạn (TTL)
 *     {"loai":"thanh_toan","ma_yc":k,"ghe":["E7","E8"]}                  ghế mình đang giữ, còn hạn -> vé
 *     {"loai":"nha","ma_yc":k,"ghe":["E7"]}                              trả lại ghế đang giữ
 * Luật: giữ được khi MỌI ghế trong yêu cầu đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); lượt giữ hết
 * hạn sau ttl_s giây (mặc định 30 s) – luồng nền của máy chủ (100 ms/lần) "dọn" ghế quá hạn, và một lượt giữ quá hạn
 * cũng bị coi là trống ngay cả khi chưa kịp dọn; thanh toán chỉ nhận ghế mình đang giữ và còn hạn (quá hạn ->
 * "het_han"). Mỗi ghế là MỘT ô của KhoTranhChap:
 *     {"tt":"trong"|"giu"|"ban","nguoi","het_han"(đồng hồ đơn điệu),"ma_ve","nguoi_het_han"}
 * nên khóa theo từng ghế: giữ nhiều ghế = khóa nhiều ô theo thứ tự tên ô (không khóa chết).
 *
 * Sổ kiểm toán đếm hai sai phạm:
 *   trung_ghe – một ghế được giữ/bán cho người thứ hai khi đang được người khác giữ (còn hạn) hoặc đã bán;
 *   tt_sai    – thanh toán được chấp nhận cho ghế không do mình giữ hoặc lượt giữ đã hết hạn.
 */
final class MayChu {
    private MayChu() {}

    static final String HANG = "ABCDEFGH";
    static final int COT = 12;
    static final List<String> TAT_CA_GHE = new ArrayList<>();
    static {
        for (char r : HANG.toCharArray()) for (int c = 1; c <= COT; c++) TAT_CA_GHE.add("" + r + c);
    }
    static final Set<String> GHE_HOP_LE = new HashSet<>(TAT_CA_GHE);
    static final int TOI_DA_GHE = 8;                    // một yêu cầu tối đa 8 ghế

    /** Kiểm tra danh sách ghế: 1–8 mã ghế hợp lệ, không trùng; trả về List hoặc null. */
    static List<String> dsGhe(Object x) {
        if (!(x instanceof List<?> l) || l.isEmpty() || l.size() > TOI_DA_GHE) return null;
        Set<String> khac = new HashSet<>();
        List<String> kq = new ArrayList<>();
        for (Object g : l) {
            khac.add(Chung.py(g));
            if (!(g instanceof String s) || !GHE_HOP_LE.contains(s)) return null;
            kq.add(s);
        }
        return khac.size() == l.size() ? kq : null;
    }

    static Map<String, Object> trong(Object lanCuoi) {
        return Json.obj("tt", "trong", "nguoi", null, "het_han", null, "ma_ve", null, "nguoi_het_han", lanCuoi);
    }

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("trung_ghe", "ghế trùng");
        m.put("tt_sai", "thanh toán sai");
        return m;
    }

    static final class SoKiemToanDatVe extends Chung.SoKiemToan {
        final Map<String, Map<String, Object>> ghe = new LinkedHashMap<>();   // theo sổ: ghế -> {tt, nguoi, het_han}
        long soVe;                                                          // số vé đã bán theo sổ

        SoKiemToanDatVe() { super(tenSaiPham()); }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            Object nguoi = gd.get("nguoi");
            double luc = Chung.sod(gd.get("luc"));
            String loai = (String) gd.get("loai");
            for (Object o : Chung.list(gd.get("ghe"))) {
                String g = (String) o;
                Map<String, Object> cu = ghe.get(g);
                if (loai.equals("giu")) {
                    if (cu != null && "ban".equals(cu.get("tt"))) {
                        saiPham("trung_ghe", g + " giữ cho " + nguoi + " khi đã bán cho " + cu.get("nguoi"));
                        continue;
                    }
                    if (cu != null && !cu.get("nguoi").equals(nguoi) && luc < Chung.sod(cu.get("het_han")))
                        saiPham("trung_ghe", g + " giữ cho " + nguoi + " khi " + cu.get("nguoi") + " đang giữ");
                    ghe.put(g, Json.obj("tt", "giu", "nguoi", nguoi, "het_han", gd.get("het_han")));
                } else if (loai.equals("ban")) {
                    soVe++;
                    if (cu != null && "ban".equals(cu.get("tt"))) {
                        saiPham("trung_ghe", g + " bán cho " + nguoi + " khi đã bán cho " + cu.get("nguoi"));
                        continue;
                    }
                    if (cu == null || !cu.get("nguoi").equals(nguoi))
                        saiPham("tt_sai", g + ": " + nguoi + " trả tiền ghế không do mình giữ");
                    else if (luc >= Chung.sod(cu.get("het_han")))
                        saiPham("tt_sai", g + ": " + nguoi + " trả tiền khi đã hết hạn giữ");
                    ghe.put(g, Json.obj("tt", "ban", "nguoi", nguoi, "het_han", null));
                } else if (cu != null && "giu".equals(cu.get("tt")) && cu.get("nguoi").equals(nguoi)) {   // nhả / hết hạn
                    ghe.remove(g);
                }
            }
        }
    }

    static Map<String, Object> thamSoMacDinh() {
        return Json.obj("ttl_s", 30.0, "ghe_nong", "E7", "gia_ve", 90_000L, "phim", "Hành tinh xanh", "suat", "19:30");
    }

    static final class MayChuDatVe extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        Set<String> dangDon;                       // ghế quá hạn đang được luồng nền dọn
        Set<String> nguoiThat;                     // tên người dùng thật (in nhật ký cho họ)
        AtomicLong demGhi;                         // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
        volatile long phien;

        MayChuDatVe(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Đặt vé xem phim MR6.8", List.of("giu_cho", "thanh_toan", "nha"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        // ------------------------------------------------------------ tạo / đặt lại
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            double ttl = Chung.sod(ts.getOrDefault("ttl_s", 30.0));
            if (!(ttl >= 1 && ttl <= 600)) throw new IllegalArgumentException("thời hạn giữ chỗ 1–600 s");
            Object nong = ts.getOrDefault("ghe_nong", "");
            if (!"".equals(nong) && !GHE_HOP_LE.contains(nong))
                throw new IllegalArgumentException("ghế nóng phải là mã ghế A1–H12 hoặc rỗng");
            if (Chung.sod(ts.getOrDefault("gia_ve", 1L)) <= 0) throw new IllegalArgumentException("giá vé > 0");
            if (ts.containsKey("phim")) {
                String p = String.valueOf(ts.get("phim"));
                int n = p.codePointCount(0, p.length());
                if (n < 1 || n > 40) throw new IllegalArgumentException("tên phim 1–40 ký tự");
            }
        }

        @Override
        protected void khoiTao() {
            for (String g : TAT_CA_GHE) kho.datO(g, trong(null));
            so = new SoKiemToanDatVe();
            dangDon = ConcurrentHashMap.newKeySet();
            nguoiThat = ConcurrentHashMap.newKeySet();
            if (demGhi == null) {
                demGhi = new AtomicLong();
                phien = 0;
            }
        }

        // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            List<String> ghe = dsGhe(tin.get("ghe"));
            if (ghe == null) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "ghế: danh sách 1–" + TOI_DA_GHE + " mã A1–H12"));
                return;
            }
            boolean that = khach.soNguoi == null;
            if (that) nguoiThat.add(nguoi);
            double ttl = tsD("ttl_s");
            long gia = tsL("gia_ve");
            String suat = tsS("suat");
            Consumer<Map<String, Object>> tra = kq -> {                 // bỏ các trường nội bộ ("_…") trước khi gửi
                Map<String, Object> m = new LinkedHashMap<>();
                kq.forEach((k, v) -> { if (!k.startsWith("_")) m.put(k, v); });
                traLoi.accept(m);
            };
            Object loai = tin.get("loai");
            if ("giu_cho".equals(loai)) {
                Chung.XuLy xuLy = doc -> {                              // hàm THUẦN: mọi ghế phải trống / quá hạn / của mình
                    double luc = Chung.bayGio(), conLai = ttl;
                    List<Object> moi = new ArrayList<>();
                    for (String g : ghe) {
                        Map<String, Object> v = Chung.map(doc.get(g));
                        if ("ban".equals(v.get("tt")))
                            return new Chung.Kq(Json.obj("ok", false, "ma_loi", "da_ban", "ly_do", "ghế " + g + " đã bán",
                                    "ghe_ban", new ArrayList<>(List.of(g))), null);
                        if ("giu".equals(v.get("tt")) && luc < Chung.sod(v.get("het_han"))) {
                            if (!nguoi.equals(v.get("nguoi")))
                                return new Chung.Kq(Json.obj("ok", false, "ma_loi", "dang_giu", "ly_do", "ghế " + g + " đang có người giữ",
                                        "ghe_ban", new ArrayList<>(List.of(g))), null);
                            conLai = Math.min(conLai, Chung.sod(v.get("het_han")) - luc);
                        } else moi.add(g);
                    }
                    double hetHan = luc + ttl;
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (Object g : moi)
                        ghi.put((String) g, Json.obj("tt", "giu", "nguoi", nguoi, "het_han", hetHan, "ma_ve", null, "nguoi_het_han", null));
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(ghe), "moi", moi,
                            "het_han_ms", Math.round(conLai * 1000), "nguoi", nguoi, "_luc", luc, "_het_han", hetHan),
                            ghi.isEmpty() ? null : ghi);
                };
                kho.thucHien(ghe, xuLy, tra, kq -> daGiu(kq, that));
            } else if ("thanh_toan".equals(loai)) {
                Chung.XuLy xuLy = doc -> {
                    double luc = Chung.bayGio();
                    for (String g : ghe) {
                        Map<String, Object> v = Chung.map(doc.get(g));
                        boolean cuaToi = "giu".equals(v.get("tt")) && nguoi.equals(v.get("nguoi"));
                        if (cuaToi && luc < Chung.sod(v.get("het_han"))) continue;
                        if (cuaToi || ("trong".equals(v.get("tt")) && nguoi.equals(v.get("nguoi_het_han"))))
                            return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_han", "ly_do", "giữ chỗ " + g + " đã hết hạn"), null);
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_giu", "ly_do", "ghế " + g + " không do bạn giữ"), null);
                    }
                    List<Object> ve = new ArrayList<>();
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (String g : ghe) {
                        String ma = "S" + suat.replace(":", "") + "-" + g;
                        ve.add(ma);
                        ghi.put(g, Json.obj("tt", "ban", "nguoi", nguoi, "het_han", null, "ma_ve", ma, "nguoi_het_han", null));
                    }
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(ghe), "ma_ve", ve, "tien", gia * ghe.size(),
                            "nguoi", nguoi, "_luc", luc), ghi);
                };
                kho.thucHien(ghe, xuLy, tra, kq -> daBan(kq, that));
            } else {
                Chung.XuLy xuLy = doc -> {
                    List<Object> nha = new ArrayList<>();
                    for (String g : ghe) {
                        Map<String, Object> v = Chung.map(doc.get(g));
                        if ("giu".equals(v.get("tt")) && nguoi.equals(v.get("nguoi"))) nha.add(g);
                    }
                    if (nha.isEmpty())
                        return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_giu", "ly_do", "bạn không giữ ghế nào trong số này"), null);
                    Map<String, Object> ghi = new LinkedHashMap<>();
                    for (Object g : nha) ghi.put((String) g, trong(null));
                    return new Chung.Kq(Json.obj("ok", true, "ghe", nha, "nguoi", nguoi), ghi);
                };
                kho.thucHien(ghe, xuLy, tra, kq -> daNha(kq, that));
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

        private void daGiu(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "giu", "ghe", kq.get("moi"), "nguoi", kq.get("nguoi"), "luc", kq.get("_luc"), "het_han", kq.get("_het_han")));
            if (that) thongBao(kq.get("nguoi") + " giữ " + noi(kq.get("moi")) + " (" + Chung.g(tsD("ttl_s")) + " s)");
            baoDoi();
        }

        private void daBan(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "ban", "ghe", kq.get("ghe"), "nguoi", kq.get("nguoi"), "luc", kq.get("_luc")));
            if (that) thongBao(kq.get("nguoi") + " mua " + Chung.list(kq.get("ghe")).size() + " vé " + noi(kq.get("ghe"))
                    + " (" + Chung.tien(Chung.so(kq.get("tien"))) + ")");
            baoDoi();
        }

        private void daNha(Map<String, Object> kq, boolean that) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "nha", "ghe", kq.get("ghe"), "nguoi", kq.get("nguoi")));
            if (that) thongBao(kq.get("nguoi") + " nhả " + noi(kq.get("ghe")));
            baoDoi();
        }

        // ------------------------------------------------------------ dọn ghế quá hạn (luồng nền, 100 ms)
        @Override
        protected void dinhKy(double bayGio) {
            for (String g : TAT_CA_GHE) {
                Map<String, Object> v = kho.doc(g);
                if (!"giu".equals(v.get("tt")) || bayGio < Chung.sod(v.get("het_han")) || dangDon.contains(g)) continue;
                dangDon.add(g);
                Chung.XuLy xuLy = doc -> {                             // nhả ghế quá hạn cũng là một thao tác ghi
                    Map<String, Object> x = Chung.map(doc.get(g));
                    if (!"giu".equals(x.get("tt")) || Chung.bayGio() < Chung.sod(x.get("het_han")))
                        return new Chung.Kq(Json.obj("ok", false), null);     // vừa được thanh toán / giữ lại
                    return new Chung.Kq(Json.obj("ok", true, "ghe", new ArrayList<Object>(List.of(g)), "nguoi", x.get("nguoi")),
                            Json.obj(g, trong(x.get("nguoi"))));
                };
                kho.thucHien(List.of(g), xuLy, kq -> dangDon.remove(g), this::daHetHan);
            }
        }

        private void daHetHan(Map<String, Object> kq) {
            danhSo(kq);
            so.ghi(Json.obj("loai", "het_han", "ghe", kq.get("ghe"), "nguoi", kq.get("nguoi")));
            if (nguoiThat.contains(String.valueOf(kq.get("nguoi"))))
                thongBao("Hết hạn giữ " + Chung.list(kq.get("ghe")).get(0) + " (" + kq.get("nguoi") + "), ghế trống lại");
            baoDoi();
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            long ph = phien;                                     // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
            double bayGio = Chung.bayGio();
            List<Object> soDo = new ArrayList<>();
            Map<String, Object> chu = new LinkedHashMap<>(), conLai = new LinkedHashMap<>();
            long dTrong = 0, dGiu = 0, dBan = 0;
            for (char r : HANG.toCharArray()) {
                StringBuilder dong = new StringBuilder();
                for (int c = 1; c <= COT; c++) {
                    String g = "" + r + c;
                    Map<String, Object> v = kho.doc(g);
                    if ("giu".equals(v.get("tt")) && bayGio < Chung.sod(v.get("het_han"))) {
                        dong.append('g');
                        chu.put(g, v.get("nguoi"));
                        conLai.put(g, Math.round((Chung.sod(v.get("het_han")) - bayGio) * 1000));
                        dGiu++;
                    } else if ("ban".equals(v.get("tt"))) {
                        dong.append('b');
                        chu.put(g, v.get("nguoi"));
                        dBan++;
                    } else {
                        dong.append('.');
                        dTrong++;
                    }
                }
                soDo.add(dong.toString());
            }
            return Json.obj("phien", ph, "phim", thamSo.get("phim"), "suat", thamSo.get("suat"), "gia_ve", thamSo.get("gia_ve"),
                    "ttl_s", thamSo.get("ttl_s"), "ghe_nong", thamSo.get("ghe_nong"), "hang", HANG, "cot", (long) COT,
                    "so_do", soDo, "chu", chu, "con_lai_ms", conLai, "dem", Json.obj("trong", dTrong, "giu", dGiu, "ban", dBan));
        }

        @Override
        protected String tomTat() {
            long ban = 0, giu = 0;
            double bayGio = Chung.bayGio();
            for (String g : TAT_CA_GHE) {
                Map<String, Object> v = kho.doc(g);
                if ("ban".equals(v.get("tt"))) ban++;
                if ("giu".equals(v.get("tt")) && bayGio < Chung.sod(v.get("het_han"))) giu++;
            }
            return "Đã bán " + ban + "/" + TAT_CA_GHE.size() + " ghế, đang giữ " + giu + ", sổ " + ((SoKiemToanDatVe) so).soVe + " vé";
        }
    }
}
