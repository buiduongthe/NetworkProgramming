import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.7 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba ứng dụng gọi xe An, Bình, Chi. Kịch bản bấm nút như người dùng (mọi
 * thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–3. chế độ khoa, 1 xe: An, Bình, Chi CÙNG bấm "ĐẶT XE" -> một người được xe, hai người "Hết chỗ"; người được xe
 *        hủy chuyến;
 *   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba CÙNG bấm -> cả ba "Đặt thành công – tài xế Hùng", rồi hai người thấy
 *        "Tài xế Hùng đã nhận khách khác!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
 *   5.   đặt lại 3 xe, So sánh 4 chế độ (mô phỏng 1000 người, 200 kết nối) -> bảng, khong_khoa tô đỏ;
 *   6.   đặt lại (3 xe): An, Bình, Chi cùng đặt -> mỗi người một xe, xe chạy tới điểm đón; chụp 4 cửa sổ
 *        (java.awt.Robot) ghép thành ket_qua_gui.png.
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

    static void inCuaSo(List<DatXeGui> ds) throws Exception {
        for (DatXeGui cs : ds) log("   " + cs.ten + ": " + ui(cs::chuTt));
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

    /** Ba lần bấm liền nhau trên luồng giao diện. */
    static void cungBam(List<DatXeGui> ds) throws Exception { lam(() -> { for (DatXeGui cs : ds) cs.nutDat.doClick(0); }); }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43712;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<DatXeGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                DatXeGui cs = new DatXeGui(ten, host, cong);
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
        DatXeGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa, 1 xe)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (DatXeGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        ngu(1.0);
        log("2. An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        cungBam(ds);
        cho(() -> ds.stream().noneMatch(cs -> cs.trang.equals("dang_tim")));
        ngu(0.5);
        inCuaSo(ds);
        inServer(sv);
        DatXeGui duoc = ui(() -> ds.stream().filter(cs -> cs.trang.equals("da_dat")).findFirst().orElse(an));
        ngu(1.0);
        log("3. " + duoc.ten + " bấm 'Hủy chuyến'");
        lam(() -> duoc.nutHuy.doClick(0));
        cho(() -> duoc.trang.equals("xong"));
        ngu(0.5);
        log("   " + duoc.ten + ": " + ui(duoc::chuTt));
        log("   máy chủ: " + sv.lop.thongKeDayDu().get("tom_tat"));
        log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        lam(() -> {
            sv.oTre.setText("20");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khong_khoa");
        });
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do"))));
        ngu(0.5);
        log("   An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        cungBam(ds);
        cho(() -> ds.stream().noneMatch(cs -> cs.trang.equals("dang_tim")));
        for (DatXeGui cs : ds) log("   " + cs.ten + " nhận: " + ui(cs::chuTt));
        ngu(0.6);
        log("   …sau cap_nhat của máy chủ:");
        inCuaSo(ds);
        inServer(sv);
        log("   xe 1 chở: " + Chung.map(Chung.list(sv.lop.trangThai().get("xe")).get(0)).get("khach"));
        DatXeGui nanNhan = ui(() -> ds.stream().filter(cs -> cs.trang.equals("mat_xe")).findFirst().orElse(binh));
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(nanNhan.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("5. Máy chủ: khoa, trễ 0–2 ms, đặt lại 3 xe");
        log("   So sánh 4 chế độ (1000 người)");
        lam(() -> {
            sv.oTre.setText("2");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khoa");
            sv.datThamSo(Json.obj("so_xe", 3));
        });
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));                     // đặt lại sau khi đổi chế độ xong
        lam(() -> {
            sv.nutDatLai.doClick(0);
            sv.oN.setText("1000");
        });
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("6. Đặt lại (3 xe); An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        lam(() -> sv.nutDatLai.doClick(0));
        cho(() -> ds.stream().allMatch(cs -> Chung.so(cs.tt.get("trong")) == 3));
        ngu(0.8);
        cungBam(ds);
        cho(() -> ds.stream().noneMatch(cs -> cs.trang.equals("dang_tim")));
        ngu(1.6);
        inCuaSo(ds);
        for (DatXeGui cs : ds) {
            String dong = ui(() -> cs.chuyen == null ? null : cs.nhanTx.getText().split("\\s+")[0] + " " + cs.chuyen.get("bien_so")
                    + ", đến sau " + cs.chuyen.get("phut") + " phút");
            if (dong != null) log("   " + cs.ten + ": " + dong);
        }
        log("   máy chủ: " + sv.lop.thongKeDayDu().get("tom_tat"));
        log("7. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(chi.frame))), new File(thuMuc, "ket_qua_gui.png"));
        cho(() -> ds.stream().allMatch(cs -> cs.hoatHinh == null || cs.daToi()));
        ngu(0.3);
        inCuaSo(ds);
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   Bình: " + ui(() -> binh.thanhTt.getText()));
        lam(() -> {
            for (DatXeGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
