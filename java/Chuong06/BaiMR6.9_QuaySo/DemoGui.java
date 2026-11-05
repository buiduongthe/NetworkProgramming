import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.9 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba vòng quay: An, Bình và "Bình (máy 2)" – cùng một người mở trên hai
 * thiết bị. Kịch bản bấm nút như người dùng (mọi thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ),
 * in "[demo] …":
 *   1–2. chế độ khoa, 1 món quà, p = 1: cả ba cùng bấm QUAY -> đúng một người trúng, thiết bị thứ hai của Bình bị từ
 *        chối "đã quay";
 *   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> nhiều người "trúng" một món quà -> SAI PHẠM (thừa quà,
 *        quay 2 lần); chụp ket_qua_gui_tranh_chap.png;
 *   4.   So sánh 4 chế độ (10 000 người, p = 1/20, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
 *   5–6. 10 quà, p = 0.8: An, Bình quay; Bình (máy 2) quay -> "đã quay"; bắn đồng loạt 500 người -> danh sách người
 *        trúng trực tiếp; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
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

    static void inCuaSo(List<VongQuayGui> ds) throws Exception {
        for (VongQuayGui cs : ds) log("   " + cs.nhanHien + ": " + ui(cs::thongBaoText));
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

    static void cungBam(List<VongQuayGui> ds) throws Exception {
        lam(() -> { for (VongQuayGui cs : ds) cs.nutQuay.doClick(0); });
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43732;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<VongQuayGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            String[] ten = {"An", "Bình", "Bình"};
            for (int i = 0; i < 3; i++) {
                VongQuayGui cs = new VongQuayGui(ten[i], host, cong);
                if (i == 2) {                                            // cùng người, thiết bị thứ hai
                    cs.nhanHien = "Bình (máy 2)";
                    cs.frame.setTitle("Vòng quay may mắn – Bình (máy 2)");
                }
                if (i == 1) {                                            // Bình, Bình (máy 2): hàng dưới
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
        VongQuayGui an = ds.get(0), binh = ds.get(1), binh2 = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Bình (máy 2): Kết nối");
        lam(() -> { for (VongQuayGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        log("2. Đặt lại: 1 món quà, p = 1 (lượt đầu chắc trúng)");
        datLai(sv, Json.obj("so_qua", 1, "p", 1));
        cho(() -> ds.stream().allMatch(cs -> Chung.so(cs.tt.get("so_qua")) == 1));
        ngu(0.5);
        log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
        cungBam(ds);
        ngu(0.3);
        cho(() -> ds.stream().allMatch(cs -> cs.pha == null));
        inCuaSo(ds);
        inServer(sv);
        log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
        tre(sv, 20);
        datLai(sv, Json.obj("so_qua", 1, "p", 1));
        lam(() -> sv.chonCheDo("khong_khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khong_khoa"));
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do")) && cs.nutQuay.isEnabled()));
        ngu(0.5);
        log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
        cungBam(ds);
        ngu(0.3);
        cho(() -> ds.stream().allMatch(cs -> cs.pha == null));
        ngu(0.5);
        inCuaSo(ds);
        inServer(sv);
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(binh2.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("4. Máy chủ: khoa, trễ 0–2 ms; 10 quà, p = 0.05 (1/20);");
        log("   So sánh 4 chế độ, 10000 người");
        tre(sv, 2);
        datLai(sv, Json.obj("so_qua", 10, "p", 0.05));
        lam(() -> sv.chonCheDo("khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        lam(() -> sv.oN.setText("10000"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("5. Đặt lại: 10 quà, p = 0.8; An quay, rồi Bình quay");
        datLai(sv, Json.obj("so_qua", 10, "p", 0.8));
        cho(() -> ds.stream().allMatch(cs -> Chung.sod(cs.tt.get("p")) == 0.8 && cs.nutQuay.isEnabled()));
        ngu(0.5);
        lam(() -> an.nutQuay.doClick(0));
        ngu(0.4);
        lam(() -> binh.nutQuay.doClick(0));
        ngu(0.3);
        cho(() -> an.pha == null && binh.pha == null);
        log("   Bình (máy 2) bấm QUAY");
        lam(() -> binh2.nutQuay.doClick(0));
        ngu(0.3);
        cho(() -> binh2.pha == null);
        inCuaSo(ds);
        log("6. Bắn đồng loạt 500 người (chế độ khoa)");
        lam(() -> {
            sv.oN.setText("500");
            sv.nutBan.doClick(0);
        });
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        ngu(0.8);
        lam(sv::hienLaiSoSanh);                                         // bảng: hiện lại kết quả so sánh 4 chế độ
        inServer(sv);
        log("   Người trúng gần nhất (cửa sổ An):");
        for (int i = 0; i < Math.min(4, ui(() -> an.dsTrung.size())); i++) {
            int ii = i;
            log("     " + ui(() -> an.dsTrung.get(ii)));
        }
        log("7. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(binh2.frame))), new File(thuMuc, "ket_qua_gui.png"));
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   An: " + ui(() -> an.thanhTt.getText()));
        lam(() -> {
            for (VongQuayGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
