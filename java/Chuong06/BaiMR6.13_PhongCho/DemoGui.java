import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.JLabel;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.13 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba trang bán vé An, Bình, Chi. Kịch bản bấm nút như người dùng (mọi thao
 * tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–3. khoa, phòng chờ bật (2000 vé, cấp lượt 50 người/s): Chi vào hàng đầu tiên, tới lượt ngay; 300 người ảo ùa
 *        vào hàng; An, Bình vào sau họ -> thanh tiến trình, "còn khoảng … người phía trước"; chụp
 *        ket_qua_gui_phong_cho.png; An tới lượt (đồng hồ 20 s) và mua 3 vé;
 *   4.   khong_khoa, trễ 0–20 ms: cửa sổ Chi đăng nhập lại tên "An" (tab thứ hai), hai tab cùng vào hàng, cùng được
 *        lượt, cùng bấm THANH TOÁN -> cả hai đơn được nhận -> SAI PHẠM "sai lượt"; chụp ket_qua_gui_tranh_chap.png;
 *   5.   So sánh 4 chế độ (1000 người) và So sánh bật/tắt phòng chờ -> hai bảng;
 *   6–7. 20 người/s, thùng 2: Bình, Chi vào hàng, Chi mua; 400 người ảo ùa vào, An vào hàng sau họ; chụp 4 cửa sổ
 *        ghép thành ket_qua_gui.png.
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

    static void inServer(ServerGui sv) {
        Map<String, Object> tk = sv.lop.thongKeDayDu();
        log("   máy chủ: " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", từ chối " + tk.get("tu_choi")
                + ", sai phạm " + tk.get("sai_pham_tong"));
        log("   " + tk.get("tom_tat"));
        List<Object> vd = Chung.list(tk.get("vi_du_sai_pham"));
        for (int i = 0; i < Math.min(2, vd.size()); i++) log("     ví dụ: " + vd.get(i));
    }

    static void ghi(List<List<BufferedImage>> hang, File tep) throws Exception {
        GiaoDienChung.ghep(hang, tep.getPath());
        log("   Đã ghi " + tep.getName() + " (" + tep.length() / 1024 + " KB)");
    }

    static void tre(ServerGui sv, int ms) throws Exception {
        lam(() -> {
            sv.oTre.setText(String.valueOf(ms));
            sv.nutTre.doClick(0);
        });
    }


    static void datLai(ServerGui sv, List<PhongChoGui> ds, Map<String, Object> ts) throws Exception {
        lam(() -> {
            sv.datThamSo(ts);
            sv.nutDatLai.doClick(0);
            for (PhongChoGui cs : ds) cs.veSuKien();
        });
    }

    /** Bấm "Bắn đồng loạt" với n người trên k kết nối. */
    static void ban(ServerGui sv, int n, int k) throws Exception {
        lam(() -> {
            sv.oN.setText(String.valueOf(n));
            sv.oK.setText(String.valueOf(k));
            sv.nutBan.doClick(0);
        });
    }

    static String chu(Callable<JLabel> l) throws Exception { return ui(() -> l.call().getText()); }

    static long soCap(ServerGui sv) { return Chung.so(sv.lop.trangThai().get("so_cap")); }

    static void dangNhap(PhongChoGui cs, String ten) throws Exception {
        lam(() -> {
            cs.nutRoi.doClick(0);
            cs.oTen.setText(ten);
            cs.nutKetNoi.doClick(0);
        });
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43772;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<PhongChoGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                PhongChoGui cs = new PhongChoGui(ten, host, cong);
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
        PhongChoGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (khoa, phòng chờ bật)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (PhongChoGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        ngu(0.8);
        log("2. Đặt lại: 2000 vé, cấp lượt 50 người/s, thùng 50");
        datLai(sv, ds, Json.obj("tong_ve", 2000, "toc_do", 50, "dung_luong", 50, "phong_cho", 1));
        cho(() -> Chung.so(Chung.map(chi.tt.get("su_kien")).get("tong_ve")) == 2000);
        ngu(0.4);
        log("   Chi: VÀO HÀNG CHỜ (người đầu tiên)");
        lam(() -> chi.nutVao.doClick(0));
        cho(() -> chi.trang == chi.trangMua);
        log("   Chi: " + chu(() -> chi.bangLuot));
        log("   Mô phỏng: 300 người ùa vào hàng (Bắn đồng loạt)");
        ban(sv, 300, 100);
        cho(() -> soCap(sv) >= 250);
        log("   An, Bình: VÀO HÀNG CHỜ");
        lam(() -> {
            an.nutVao.doClick(0);
            binh.nutVao.doClick(0);
        });
        cho(() -> an.trang == an.trangCho && binh.trang == binh.trangCho);
        ngu(0.5);
        log("   Chi: chọn 2 vé, THANH TOÁN");
        lam(() -> chi.mua(2));
        ngu(1.0);
        log("   Chi: " + chu(() -> chi.nhanKq));
        log("   An: " + chu(() -> an.nhanSo));
        log("   An: " + chu(() -> an.nhanTruoc));
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame)), List.of(GiaoDienChung.chup(an.frame), GiaoDienChung.chup(chi.frame))),
                new File(thuMuc, "ket_qua_gui_phong_cho.png"));
        cho(() -> an.trang == an.trangMua);
        ngu(0.5);
        log("3. An tới lượt: đồng hồ " + chu(() -> an.nhanDongHo) + "; chọn 3 vé, THANH TOÁN");
        lam(() -> an.mua(3));
        cho(() -> an.soDon == 1);
        log("   An: " + chu(() -> an.nhanKq));
        cho(() -> !sv.dangMoPhong);
        inServer(sv);
        log("4. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại (500 vé)");
        tre(sv, 20);
        lam(() -> sv.chonCheDo("khong_khoa"));
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do"))));
        datLai(sv, ds, Json.obj("tong_ve", 500));
        cho(() -> Chung.so(Chung.map(an.tt.get("su_kien")).get("tong_ve")) == 500 && Chung.so(an.tt.get("da_cap_luot")) == 0);
        log("   Chi: Rời, đăng nhập lại tên 'An' (tab thứ hai của An)");
        dangNhap(chi, "An");
        cho(() -> chi.kn != null && chi.tt != null);
        ngu(0.5);
        log("   Hai tab của An cùng VÀO HÀNG CHỜ");
        lam(() -> {
            an.nutVao.doClick(0);
            chi.nutVao.doClick(0);
        });
        cho(() -> an.trang == an.trangMua && chi.trang == chi.trangMua);
        log("   Hai tab cùng bấm THANH TOÁN 2 vé (một lượt)");
        int dem = ui(() -> an.soKq + chi.soKq);
        lam(() -> {
            an.mua(2);
            chi.mua(2);
        });
        cho(() -> an.soKq + chi.soKq == dem + 2);
        ngu(0.5);
        log("   tab 1: " + chu(() -> an.nhanKq));
        log("   tab 2: " + chu(() -> chi.nhanKq));
        inServer(sv);
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(chi.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("5. khoa, trễ 0–2 ms; Chi vào lại; So sánh 4 chế độ");
        dangNhap(chi, "Chi");
        tre(sv, 2);
        lam(() -> sv.chonCheDo("khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khoa") && chi.kn != null);
        lam(() -> {
            sv.oN.setText("1000");
            sv.oK.setText("200");
        });
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        log("   So sánh bật/tắt phòng chờ (khoa, 1000 người)");
        cho(() -> sv.nutSoSanhPc.isEnabled());                          // nút mở lại ở nhịp vẽ kế tiếp (250 ms)
        lam(() -> sv.nutSoSanhPc.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : MoPhong.bangPhongCho(sv.hangPc)) log(d);
        log("6. Đặt lại: 2000 vé, 20 người/s, thùng 2");
        datLai(sv, ds, Json.obj("tong_ve", 2000, "toc_do", 20, "dung_luong", 2, "phong_cho", 1));
        cho(() -> Chung.so(Chung.map(an.tt.get("su_kien")).get("tong_ve")) == 2000 && Chung.sod(an.tt.get("toc_do")) == 20);
        ngu(0.4);
        log("   Bình, Chi vào hàng; Chi mua 1 vé");
        lam(() -> {
            binh.nutVao.doClick(0);
            chi.nutVao.doClick(0);
        });
        cho(() -> binh.trang == binh.trangMua && chi.trang == chi.trangMua);
        lam(() -> chi.mua(1));
        log("   400 người ùa vào; An vào hàng sau họ");
        ban(sv, 400, 100);
        cho(() -> soCap(sv) >= 350);
        lam(() -> an.nutVao.doClick(0));
        cho(() -> an.trang == an.trangCho);
        ngu(2.0);
        log("   An: " + chu(() -> an.nhanTruoc));
        log("   Bình: đồng hồ " + chu(() -> binh.nhanDongHo));
        log("   Chi: " + chu(() -> chi.nhanKq));
        log("7. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(chi.frame))), new File(thuMuc, "ket_qua_gui.png"));
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   Bình: " + ui(() -> binh.thanhTt.getText()));
        lam(() -> {
            for (PhongChoGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
