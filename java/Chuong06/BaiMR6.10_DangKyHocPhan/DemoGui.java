import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.10 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba cổng đăng ký An, Bình, Chi. Kịch bản bấm nút như người dùng (mọi thao
 * tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–2. chế độ khoa, 3 lớp sĩ số 1: cả ba CÙNG bấm Đăng ký lớp 123013-01 -> đúng một người được, hai người "lớp đã
 *        đủ";
 *   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác -> cả ba "thành công", rồi danh sách lớp chỉ còn một tên: hai
 *        người thấy "Đăng ký … đã bị MẤT" -> SAI PHẠM (vượt sĩ số); chụp ket_qua_gui_tranh_chap.png;
 *   4.   So sánh 4 chế độ (2000 sinh viên ảo, 3 lớp × 50) -> bảng;
 *   5–6. An đăng ký 2 lớp, Bình 1 lớp; 500 sinh viên ảo lấp đầy; Chi thấy "Đã đủ"; An hủy một lớp, Chi đăng ký được
 *        chỗ vừa trống; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
 *
 * Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" java -cp out DemoGui [host] [cổng]
 */
public class DemoGui {
    static final String LOP1 = "123013-01", LOP2 = "123005-01";

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

    static void inCuaSo(List<DangKyGui> ds) throws Exception {
        for (DangKyGui cs : ds) log("   " + cs.ten + ": " + ui(cs::thongBaoText));
    }

    static void inServer(ServerGui sv) {
        Map<String, Object> tk = sv.lop.thongKeDayDu();
        log("   máy chủ: " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", từ chối " + tk.get("tu_choi")
                + ", sai phạm " + tk.get("sai_pham_tong"));
        List<Object> vd = Chung.list(tk.get("vi_du_sai_pham"));
        for (int i = 0; i < Math.min(2, vd.size()); i++) log("     ví dụ: " + vd.get(i));
        log("   " + tk.get("tom_tat"));
    }

    static void inLop(ServerGui sv) {
        for (Object o : Chung.list(sv.lop.trangThai().get("lop"))) {
            Map<String, Object> x = Chung.map(o);
            List<Object> svs = Chung.list(x.get("sv"));
            List<String> ba = new ArrayList<>();
            for (int i = 0; i < Math.min(3, svs.size()); i++) ba.add(String.valueOf(svs.get(i)));
            log(("   " + x.get("ma") + ": " + x.get("da_dk") + "/" + x.get("si_so") + " " + String.join(" ", ba)).stripTrailing());
        }
    }

    static void ghi(List<List<BufferedImage>> hang, File tep, String them) throws Exception {
        GiaoDienChung.ghep(hang, tep.getPath());
        log("   Đã ghi " + tep.getName() + " (" + tep.length() / 1024 + " KB" + them + ")");
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

    static void xong(List<DangKyGui> ds) throws Exception { cho(() -> ds.stream().allMatch(cs -> cs.dangCho.isEmpty())); }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43742;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<DangKyGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                DangKyGui cs = new DangKyGui(ten, host, cong);
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
        DangKyGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (DangKyGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        log("2. Đặt lại: 3 lớp, sĩ số 1 (còn đúng 1 chỗ mỗi lớp)");
        datLai(sv, Json.obj("so_lop", 3, "si_so", 1));
        cho(() -> ds.stream().allMatch(cs -> Chung.so(cs.tt.get("si_so")) == 1));
        ngu(0.5);
        log("   An, Bình, Chi CÙNG bấm Đăng ký " + LOP1);
        lam(() -> { for (DangKyGui cs : ds) cs.dangKy(LOP1); });
        xong(ds);
        ngu(0.8);
        inCuaSo(ds);
        inServer(sv);
        log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
        tre(sv, 20);
        datLai(sv, Json.obj("so_lop", 3, "si_so", 1));
        lam(() -> sv.chonCheDo("khong_khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khong_khoa"));
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do")) && cs.cuaToi.isEmpty()));
        ngu(0.5);
        log("   An, Bình, Chi CÙNG bấm Đăng ký " + LOP1);
        lam(() -> { for (DangKyGui cs : ds) cs.dangKy(LOP1); });
        xong(ds);
        log("   ngay khi có trả lời:");
        inCuaSo(ds);
        ngu(1.2);
        log("   1 giây sau (đối chiếu danh sách lớp):");
        inCuaSo(ds);
        inServer(sv);
        inLop(sv);
        DangKyGui mat = ui(() -> ds.stream().filter(cs -> cs.thongBaoText().contains("MẤT")).findFirst().orElse(binh));
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(mat.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"), ", cửa sổ " + mat.ten);
        log("4. Máy chủ: khoa, trễ 0–2 ms, 3 lớp × 50; So sánh");
        tre(sv, 2);
        datLai(sv, Json.obj("so_lop", 3, "si_so", 50));
        lam(() -> sv.chonCheDo("khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("5. Đặt lại; An đăng ký 2 lớp, Bình 1 lớp");
        datLai(sv, Json.obj("so_lop", 3, "si_so", 50));
        cho(() -> ds.stream().allMatch(cs -> cs.cuaToi.isEmpty() && cs.hang.get(LOP1).nut.getText().equals("Đăng ký")));
        ngu(0.5);
        lam(() -> {
            an.dangKy(LOP1);
            an.dangKy(LOP2);
            binh.dangKy(LOP1);
        });
        xong(ds);
        ngu(0.5);
        log("   Bắn đồng loạt 500 sinh viên ảo (mỗi người 1 lớp)");
        lam(() -> {
            sv.oN.setText("500");
            sv.nutBan.doClick(0);
        });
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        ngu(0.8);
        lam(sv::hienLaiSoSanh);                                         // bảng: hiện lại kết quả so sánh 4 chế độ
        inLop(sv);
        log("   Chi bấm '" + ui(() -> chi.hang.get(LOP2).nut.getText()) + "' ở " + LOP2);
        lam(() -> chi.dangKy(LOP2));
        xong(ds);
        ngu(0.5);
        inCuaSo(List.of(chi));
        log("6. An bấm Hủy " + LOP2 + ", rồi Chi bấm Đăng ký " + LOP2);
        lam(() -> an.dangKy(LOP2));
        xong(ds);
        cho(() -> chi.hang.get(LOP2).nut.getText().equals("Đăng ký"));
        lam(() -> chi.dangKy(LOP2));
        xong(ds);
        ngu(0.6);
        inCuaSo(ds);
        inServer(sv);
        log("7. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(chi.frame))), new File(thuMuc, "ket_qua_gui.png"), "");
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   An: " + ui(() -> an.thanhTt.getText()));
        lam(() -> {
            for (DangKyGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
