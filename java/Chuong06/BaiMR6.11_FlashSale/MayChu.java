import java.time.LocalTime;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.11 – Lõi máy chủ flash sale: nghiệp vụ + sổ kiểm toán (khung ở Chung.java).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
 * "Giờ vàng" flash sale: kho có 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm "MUA NGAY" đúng
 * lúc mở. Thao tác ghi duy nhất:
 *     {"loai":"mua","ma_yc":k,"so_luong":1|2[,"nguoi":"u17"]}
 * Luật: chưa tới giờ mở bán -> "chua_mo" (kèm con_ms); mỗi đơn 1–toi_da (mặc định 2) sản phẩm; tồn kho phải đủ (còn 1
 * mà mua 2 -> "khong_du"); hết -> "het_hang".
 *
 * Trạng thái là MỘT ô "kho" của KhoTranhChap: {"ton","da_ban","so_don"} – điểm nóng mà mọi đơn hàng cùng tranh. Hai
 * việc KHÔNG cần đi qua vùng găng: kiểm tra giờ mở bán (chỉ đọc đồng hồ) và "hết hàng nhanh" (đọc tồn kho không khóa:
 * tồn chỉ giảm, nên đã thấy 0 thì chắc chắn là 0) – giúp hàng nghìn yêu cầu đến sau khi cháy hàng được trả lời ngay
 * mà không phải xếp hàng chờ khóa.
 *
 * Sổ kiểm toán (tổng đã bán theo sổ, ghi trong vùng găng) đếm hai sai phạm:
 *   ton_am  – một đơn được chấp nhận làm tổng đã bán vượt tồn đầu kỳ (tồn kho thật bị âm – bán vượt);
 *   lech_so – cuối buổi: tổng đã bán theo sổ ≠ tồn đầu kỳ − tồn hiện tại (đếm số sản phẩm lệch).
 */
final class MayChu {
    private MayChu() {}

    static Map<String, String> tenSaiPham() {
        Map<String, String> m = new LinkedHashMap<>();
        m.put("ton_am", "tồn âm");
        m.put("lech_so", "lệch sổ");
        return m;
    }

    static final class SoKiemToanFlashSale extends Chung.SoKiemToan {
        final long tonDau;
        long tongBan;

        SoKiemToanFlashSale(long tonDau) {
            super(tenSaiPham());
            this.tonDau = tonDau;
        }

        @Override
        protected void kiemTra(Map<String, Object> gd) {
            long sl = Chung.so(gd.get("so_luong"));
            tongBan += sl;
            if (tongBan > tonDau) saiPham("ton_am", gd.get("nguoi") + " mua " + sl + ": bán " + tongBan + "/" + tonDau);
        }

        @Override
        protected Map<String, Long> kiemTraCuoi(Map<String, Object> doc) {
            Object k = doc.get("kho");
            if (k == null) return Map.of();
            long lech = Math.abs((tonDau - Chung.so(Chung.map(k).get("ton"))) - tongBan);
            return lech != 0 ? Map.of("lech_so", lech) : Map.of();
        }

        synchronized long tongBan() { return tongBan; }

        synchronized long soDon() { return giaoDich.size(); }
    }

    static Map<String, Object> thamSoMacDinh() {
        return Json.obj("ten_sp", "Tai nghe không dây TWS Pro", "gia_goc", 1_990_000L, "gia_sale", 199_000L,
                "ton_dau", 100L, "mo_sau_s", 30.0, "toi_da", 2L);
    }

    static final class MayChuFlashSale extends Chung.MayChuCoSo {
        // các trường dưới đây do khoiTao() đặt – KHÔNG khởi tạo ở chỗ khai báo
        volatile double moLuc;                                 // thời điểm mở bán (đồng hồ đơn điệu)
        ArrayDeque<Map<String, Object>> donGan;                // đơn mới nhất trước (để hiển thị), tối đa 30
        volatile boolean daBaoMo, daBaoHet;

        MayChuFlashSale(String cheDo, double treMs, Consumer<String> bao, Map<String, Object> thamSo) {
            super("Flash sale MR6.11", List.of("mua"), thamSoMacDinh(), cheDo, treMs, bao);
            khoiDong(thamSo);
        }

        SoKiemToanFlashSale so() { return (SoKiemToanFlashSale) so; }

        // ------------------------------------------------------------ tạo / đặt lại đợt sale
        @Override
        protected void kiemThamSo(Map<String, Object> ts) {
            super.kiemThamSo(ts);
            long ton = Chung.so(ts.getOrDefault("ton_dau", 1L)), toiDa = Chung.so(ts.getOrDefault("toi_da", 1L));
            if (ton < 1 || ton > 1_000_000) throw new IllegalArgumentException("tồn đầu kỳ 1–1 000 000");
            if (Chung.sod(ts.getOrDefault("mo_sau_s", 0.0)) < 0) throw new IllegalArgumentException("mo_sau_s ≥ 0");
            if (toiDa < 1 || toiDa > 10) throw new IllegalArgumentException("tối đa mỗi đơn 1–10");
            if (Chung.so(ts.getOrDefault("gia_sale", 1L)) <= 0 || Chung.so(ts.getOrDefault("gia_goc", 1L)) <= 0)
                throw new IllegalArgumentException("giá > 0");
        }

        @Override
        protected void khoiTao() {
            kho.datO("kho", Json.obj("ton", tsL("ton_dau"), "da_ban", 0L, "so_don", 0L));
            so = new SoKiemToanFlashSale(tsL("ton_dau"));
            moLuc = Chung.bayGio() + tsD("mo_sau_s");
            donGan = new ArrayDeque<>();
            daBaoMo = tsD("mo_sau_s") <= 0;
            daBaoHet = false;
        }

        /** Đọc ô "kho" không khóa (hiển thị, "hết hàng nhanh"); đang đặt lại -> null. */
        Map<String, Object> docKho() {
            try {
                return kho.doc("kho");
            } catch (NullPointerException e) {
                return null;
            }
        }

        // ------------------------------------------------------------ mua
        @Override
        protected void xuLyYeuCau(Chung.KhachKetNoi khach, String nguoi, Map<String, Object> tin,
                                  Consumer<Map<String, Object>> traLoi) {
            Object x = tin.get("so_luong");
            long toiDa = tsL("toi_da");
            if (!Chung.laSoNguyen(x) || Chung.so(x) < 1 || Chung.so(x) > toiDa) {
                traLoi.accept(Json.obj("ok", false, "ma_loi", "sai_du_lieu", "ly_do", "số lượng 1–" + toiDa));
                return;
            }
            long sl = Chung.so(x), gia = tsL("gia_sale");
            double con = moLuc - Chung.bayGio();
            if (con > 0) {                                            // chưa tới giờ: chỉ đọc đồng hồ, không chạm kho
                traLoi.accept(Json.obj("ok", false, "ma_loi", "chua_mo", "ly_do", Chung.f("chưa mở bán, còn %.1f s", con),
                        "con_ms", Math.round(con * 1000)));
                return;
            }
            Map<String, Object> k0 = docKho();
            if (k0 != null && Chung.so(k0.get("ton")) <= 0) {         // hết hàng nhanh: tồn chỉ giảm, đọc không khóa
                traLoi.accept(Json.obj("ok", false, "ma_loi", "het_hang", "ly_do", "hết hàng", "ton", 0L));
                return;
            }
            Chung.XuLy xuLy = doc -> {                                 // hàm THUẦN: đọc doc, trả về Kq(kq, ghi)
                Map<String, Object> k = Chung.map(doc.get("kho"));
                long ton = Chung.so(k.get("ton"));
                if (ton <= 0)
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "het_hang", "ly_do", "hết hàng", "ton", 0L), null);
                if (ton < sl)
                    return new Chung.Kq(Json.obj("ok", false, "ma_loi", "khong_du", "ly_do", "chỉ còn " + ton + " sản phẩm",
                            "ton", ton), null);
                Map<String, Object> moi = Json.obj("ton", ton - sl, "da_ban", Chung.so(k.get("da_ban")) + sl,
                        "so_don", Chung.so(k.get("so_don")) + 1);
                return new Chung.Kq(Json.obj("ok", true, "ma_don", Chung.f("FS%05d", Chung.so(moi.get("so_don"))),
                        "so_luong", sl, "ton", moi.get("ton"), "nguoi", nguoi, "thanh_tien", sl * gia), Json.obj("kho", moi));
            };
            boolean gop = khach.soNguoi != null;
            kho.thucHien(List.of("kho"), xuLy, traLoi, kq -> daBan(kq, gop));
        }

        /** Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. */
        private void daBan(Map<String, Object> kq, boolean gop) {
            so.ghi(Json.obj("nguoi", kq.get("nguoi"), "so_luong", kq.get("so_luong"), "ma_don", kq.get("ma_don")));
            synchronized (donGan) {
                donGan.addFirst(Json.obj("nguoi", kq.get("nguoi"), "so_luong", kq.get("so_luong"),
                        "gio", LocalTime.now().format(GiaoDienChung.GIO)));
                if (donGan.size() > 30) donGan.removeLast();
            }
            if (!gop) thongBao(kq.get("nguoi") + " mua " + kq.get("so_luong") + " – đơn " + kq.get("ma_don") + ", còn " + kq.get("ton"));
            if (Chung.so(kq.get("ton")) == 0 && !daBaoHet) {
                daBaoHet = true;
                thongBao(Chung.f("HẾT HÀNG sau %.2f s mở bán", Chung.bayGio() - moLuc));
            }
            baoDoi();
        }

        // ------------------------------------------------------------ đồng hồ mở bán (luồng nền, 100 ms)
        @Override
        protected void dinhKy(double bayGio) {
            if (!daBaoMo && bayGio >= moLuc) {
                daBaoMo = true;
                thongBao("MỞ BÁN! " + tsL("ton_dau") + " sản phẩm");
                baoDoi();
            }
        }

        // ------------------------------------------------------------ trạng thái gửi client
        @Override
        protected Map<String, Object> trangThaiNghiepVu() {
            Map<String, Object> k = docKho();
            if (k == null) k = Json.obj("ton", tsL("ton_dau"), "da_ban", 0L, "so_don", 0L);
            double con = moLuc - Chung.bayGio();
            List<Object> gan;
            synchronized (donGan) { gan = new ArrayList<>(donGan); }
            return Json.obj("sp", Json.obj("ten", thamSo.get("ten_sp"), "gia_goc", thamSo.get("gia_goc"),
                            "gia_sale", thamSo.get("gia_sale"), "toi_da", thamSo.get("toi_da")),
                    "ton_dau", thamSo.get("ton_dau"), "ton", k.get("ton"), "da_ban", k.get("da_ban"), "so_don", k.get("so_don"),
                    "ban_theo_so", so().tongBan(), "mo_ban", con <= 0, "mo_con_ms", Math.max(0, Math.round(con * 1000)),
                    "don_gan", new ArrayList<>(gan.subList(0, Math.min(10, gan.size()))));
        }

        @Override
        protected String tomTat() {
            Map<String, Object> k = docKho();
            return "Bán " + so().tongBan() + "/" + tsL("ton_dau") + " sp (" + so().soDon() + " đơn), tồn "
                    + (k == null ? "–" : k.get("ton"));
        }
    }
}
