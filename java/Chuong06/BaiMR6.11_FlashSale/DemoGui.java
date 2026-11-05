import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.11 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba trang flash sale An, Bình, Chi. Kịch bản bấm nút như người dùng (mọi
 * thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–2. chế độ khoa, kho 3 sản phẩm, mở bán sau 3 s: nút "CHỜ MỞ BÁN" bị khóa; hết đếm ngược, cả ba CÙNG bấm MUA
 *        NGAY (2 + 1 + 2 sản phẩm) -> bán đúng 3, người còn lại "chỉ còn … sản phẩm" / "HẾT HÀNG";
 *   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác -> cả ba "Đặt hàng thành công" -> SAI PHẠM (tồn âm, lệch sổ);
 *        chụp ket_qua_gui_tranh_chap.png;
 *   4.   So sánh 4 chế độ (10 000 người ảo, kho 100, trễ 0–1 ms) -> bảng;
 *   5–6. kho 100, mở bán sau 4 s: chụp Chi lúc đếm ngược; mở bán: An mua 2, 50 người ảo cùng mua, Bình mua 2; chụp
 *        4 cửa sổ ghép thành ket_qua_gui.png.
 *
 * Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" java -cp out DemoGui [host] [cổng]
 */
public class DemoGui {
    static <T> T ui(Callable<T> f) throws Exception {
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> { try { r.set(f.call()); } catch (Exception e) { throw new RuntimeException(e); } });
        return r.get();
    }

    static void lam(Runnable f) throws Exception { SwingUtilities.invokeAndWait(f); }

    static void cho(Callable<Boolean> dk) throws Exception {
        for (int i = 0; i < 2400; i++) {                                // tối đa 120 s
            if (ui(dk)) return;
            Thread.sleep(50);
        }
        log("Kịch bản chờ quá lâu – dừng");
        System.exit(1);
    }

    static void ngu(double giay) throws InterruptedException { Thread.sleep((long) (giay * 1000)); }

    static void log(String s) { Chung.inDong("demo", s); }

    static void inCuaSo(List<FlashSaleGui> ds) throws Exception {
        for (FlashSaleGui cs : ds) log("   " + cs.ten + ": " + ui(cs::thongBaoText));
    }

    static void inServer(ServerGui sv) {
        Map<String, Object> tk = sv.lop.thongKeDayDu();
        log("   máy chủ: " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", từ chối " + tk.get("tu_choi")
                + ", sai phạm " + tk.get("sai_pham_tong"));
        List<Object> vd = Chung.list(tk.get("vi_du_sai_pham"));
        for (int i = 0; i < Math.min(2, vd.size()); i++) log("     ví dụ: " + vd.get(i));
        log("   " + tk.get("tom_tat"));
    }

    static void ghi(List<List<BufferedImage>> hang, File tep) throws Exception {
        GiaoDienChung.ghep(hang, tep.getPath());
        log("   Đã ghi " + tep.getName() + " (" + tep.length() / 1024 + " KB)");
    }

    static void datLai(ServerGui sv, Map<String, Object> ts) throws Exception {
        lam(() -> {
            sv.datThamSo(ts);
            sv.nutDatLai.doClick(0);
        });
    }

    static void tre(ServerGui sv, int ms) throws Exception {
        lam(() -> {
            sv.oTre.setText(String.valueOf(ms));
            sv.nutTre.doClick(0);
        });
    }


    static String trangThaiNut(FlashSaleGui cs) { return cs.nutMua.isEnabled() ? "normal" : "disabled"; }

    static void cungMua(List<FlashSaleGui> ds) throws Exception {
        lam(() -> {
            int[] sl = {2, 1, 2};
            for (int i = 0; i < ds.size(); i++) ds.get(i).mua(sl[i]);
        });
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43752;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<FlashSaleGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                FlashSaleGui cs = new FlashSaleGui(ten, host, cong);
                if (ten.equals("Bình")) {                                // Bình, Chi: hàng dưới
                    x = 0;
                    y = Math.max(s.frame.getHeight(), ds.get(0).frame.getHeight()) + 40;
                }
                cs.frame.setLocation(x, y);
                cs.frame.setVisible(true);
                x += cs.frame.getWidth() + 20;
                ds.add(cs);
            }
            rs.set(s);
        });
        ServerGui sv = rs.get();
        FlashSaleGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (FlashSaleGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        log("2. Đặt lại: kho 3 sản phẩm, mở bán sau 3 s");
        datLai(sv, Json.obj("ton_dau", 3, "mo_sau_s", 3));
        cho(() -> ds.stream().allMatch(cs -> Chung.so(cs.tt.get("ton_dau")) == 3));
        ngu(0.5);
        log("   An thấy: " + ui(() -> an.nhanDongHo.getText()) + ", nút '" + ui(() -> an.nutMua.getText()) + "' ("
                + ui(() -> trangThaiNut(an)) + ")");
        cho(() -> ds.stream().allMatch(cs -> cs.nutMua.isEnabled()));
        log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
        cungMua(ds);
        cho(() -> ds.stream().noneMatch(cs -> cs.dangCho));
        ngu(0.5);
        inCuaSo(ds);
        inServer(sv);
        log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; đặt lại");
        tre(sv, 20);
        datLai(sv, Json.obj("ton_dau", 3, "mo_sau_s", 2));
        lam(() -> sv.chonCheDo("khong_khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khong_khoa"));
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do")) && cs.nutMua.isEnabled()));
        log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
        cungMua(ds);
        cho(() -> ds.stream().noneMatch(cs -> cs.dangCho));
        ngu(0.6);
        inCuaSo(ds);
        inServer(sv);
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(chi.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("4. Máy chủ: khoa, trễ 0–1 ms, kho 100; So sánh 4 chế độ");
        tre(sv, 1);
        datLai(sv, Json.obj("ton_dau", 100, "mo_sau_s", 30));
        lam(() -> sv.chonCheDo("khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("5. Đặt lại: kho 100, mở bán sau 4 s");
        datLai(sv, Json.obj("ton_dau", 100, "mo_sau_s", 4));
        cho(() -> ds.stream().allMatch(cs -> Chung.so(cs.tt.get("ton")) == 100 && !Chung.dung(cs.tt.get("mo_ban"))));
        ngu(1.5);
        log("   Chi thấy: " + ui(() -> chi.nhanTrangThai.getText()) + " " + ui(() -> chi.nhanDongHo.getText())
                + ", nút '" + ui(() -> chi.nutMua.getText()) + "'");
        BufferedImage anhChi = GiaoDienChung.chup(chi.frame);
        cho(() -> an.nutMua.isEnabled());
        log("6. Mở bán: An mua 2; 50 người ảo cùng mua; Bình mua 2");
        lam(() -> an.mua(2));
        cho(() -> !an.dangCho);
        lam(() -> {
            sv.oN.setText("50");
            sv.nutBan.doClick(0);
        });
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        lam(sv::hienLaiSoSanh);                                         // bảng: hiện lại kết quả so sánh 4 chế độ
        lam(() -> binh.mua(2));
        cho(() -> !binh.dangCho);
        ngu(0.6);
        inCuaSo(List.of(an, binh));
        inServer(sv);
        log("7. Chụp cửa sổ (Chi: lúc đếm ngược)");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), anhChi)), new File(thuMuc, "ket_qua_gui.png"));
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   An: " + ui(() -> an.thanhTt.getText()));
        lam(() -> {
            for (FlashSaleGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
