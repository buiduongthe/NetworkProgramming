import java.awt.image.BufferedImage;
import java.io.File;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.6 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba phòng đấu giá An, Bình, Chi. Kịch bản bấm nút như người dùng (mọi
 * thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–3. chế độ khoa: Bình và Chi cùng bấm "+1 bước" -> một người bị từ chối "giá đã thay đổi";
 *   4.   chế độ khong_khoa, trễ 0–20 ms: ba người cùng bấm "+1 bước" -> cả ba cùng "được chấp nhận" -> SAI PHẠM;
 *        chụp ket_qua_gui_tranh_chap.png;
 *   5.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
 *   6–7. phiên 8 s, gia hạn 4 s: trả giá phút cuối -> gia hạn hai lần -> kết thúc, công bố người thắng;
 *        chụp 4 cửa sổ (java.awt.Robot) ghép thành ket_qua_gui.png.
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
        for (int i = 0; i < 1800; i++) {                                // tối đa 90 s
            if (ui(dk)) return;
            Thread.sleep(50);
        }
        log("Kịch bản chờ quá lâu – dừng");
        System.exit(1);
    }

    static void ngu(double giay) throws InterruptedException { Thread.sleep((long) (giay * 1000)); }

    static void log(String s) { Chung.inDong("demo", s); }

    static void inCuaSo(List<DauGiaGui> ds) throws Exception {
        for (DauGiaGui cs : ds) log("   " + cs.ten + ": " + ui(cs::thongBaoText));
    }

    static void inServer(ServerGui sv) {
        Map<String, Object> tk = sv.lop.thongKeDayDu();
        log("   máy chủ: " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", từ chối " + tk.get("tu_choi")
                + ", sai phạm " + tk.get("sai_pham_tong"));
        List<Object> vd = Chung.list(tk.get("vi_du_sai_pham"));
        for (int i = 0; i < Math.min(2, vd.size()); i++) log("     ví dụ: " + vd.get(i));
    }

    static void ghi(List<List<BufferedImage>> hang, File tep) throws Exception {
        GiaoDienChung.ghep(hang, tep.getPath());
        log("   Đã ghi " + tep.getName() + " (" + tep.length() / 1024 + " KB)");
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43702;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<DauGiaGui> ds = new java.util.ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                DauGiaGui cs = new DauGiaGui(ten, host, cong);
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
        DauGiaGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa, phiên 120 s)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (DauGiaGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        ngu(1.0);
        log("2. An bấm '+1 bước'");
        lam(() -> an.nutBuoc(0).doClick(0));
        ngu(0.8);
        log("3. Bình và Chi CÙNG bấm '+1 bước' (cùng giá)");
        lam(() -> { binh.nutBuoc(0).doClick(0); chi.nutBuoc(0).doClick(0); });
        ngu(0.8);
        inCuaSo(ds);
        inServer(sv);
        log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        lam(() -> {
            sv.oTre.setText("20");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khong_khoa");
        });
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do"))));
        ngu(0.5);
        log("   An, Bình, Chi CÙNG bấm '+1 bước'");
        lam(() -> { for (DauGiaGui cs : ds) cs.nutBuoc(0).doClick(0); });
        ngu(0.8);
        inCuaSo(ds);
        inServer(sv);
        log("   Lịch sử trả giá (mới nhất trước):");
        List<Object> lsu = Chung.list(sv.lop.trangThai().get("lich_su"));
        for (int i = 0; i < Math.min(4, lsu.size()); i++) {
            Map<String, Object> x = Chung.map(lsu.get(i));
            log(Chung.f("     %s %-5s %s", x.get("gio"), x.get("nguoi"), Chung.tien(Chung.so(x.get("gia")))));
        }
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(binh.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("5. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
        lam(() -> {
            sv.oTre.setText("2");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khoa");
            sv.oN.setText("500");
        });
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("6. Đặt lại: phiên 8 s, gia hạn 4 s; An '+1 bước'");
        lam(() -> {
            sv.datThamSo(Json.obj("thoi_gian_s", 8, "gia_han_s", 4));
            sv.nutDatLai.doClick(0);
        });
        ngu(1.0);
        lam(() -> an.nutBuoc(0).doClick(0));
        ngu(4.2);
        log("   Bình '+2 bước' khi còn " + ui(() -> an.nhanDongHo.getText()));
        lam(() -> binh.nutBuoc(1).doClick(0));
        ngu(0.5);
        log("   -> đồng hồ An: " + ui(() -> an.nhanDongHo.getText() + " " + an.nhanGiaHan.getText()));
        cho(() -> an.hetHan != null && an.hetHan - Chung.bayGio() < 0.8);
        log("7. An '+1 bước' khi còn " + ui(() -> an.nhanDongHo.getText()));
        lam(() -> an.nutBuoc(0).doClick(0));
        ngu(0.5);
        log("   -> đồng hồ Bình: " + ui(() -> binh.nhanDongHo.getText() + " " + binh.nhanGiaHan.getText()));
        cho(() -> Chung.dung(an.tt.get("ket_thuc")));
        ngu(0.8);
        inCuaSo(ds);
        log("   máy chủ: " + sv.lop.thongKeDayDu().get("tom_tat"));
        log("8. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(chi.frame))), new File(thuMuc, "ket_qua_gui.png"));
        log("9. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   Bình: " + ui(() -> binh.thanhTt.getText()));
        lam(() -> {
            for (DauGiaGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
