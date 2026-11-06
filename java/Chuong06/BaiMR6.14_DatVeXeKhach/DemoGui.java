import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.14 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba ứng dụng đặt vé An, Bình, Chi. Kịch bản chọn đoạn, bấm chỗ, bấm nút như
 * người dùng (mọi thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–3. xe giường nằm, chế độ khoa: An và Bình chọn D01 Sài Gòn → Đà Lạt, Chi chọn D01 Đà Lạt → Nha Trang, cả ba CÙNG
 *        bấm "GIỮ CHỖ" -> một trong An/Bình giữ được, Chi CŨNG giữ được (đoạn không chồng nhau); hai người giữ được
 *        thanh toán -> cùng một giường bán hai vé;
 *   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn D02 Sài Gòn → Nha Trang, CÙNG bấm -> cả ba "Đã giữ", rồi hai người
 *        thấy "Chỗ D02 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
 *   5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng D01, đoạn ngẫu nhiên, 200 kết nối) -> bảng;
 *   6.   đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s: An giữ 05, 06 Sài Gòn → Bảo Lộc, không thanh toán; Bình bấm 06 cho
 *        Sài Gòn → Đà Lạt: đang giữ; hết hạn -> Bình giữ và mua 05, 06 Sài Gòn → Đà Lạt; Chi mua 05 Đà Lạt → Nha Trang;
 *        An giữ 10, 11 (đồng hồ chạy), Chi chọn 21; chụp 4 cửa sổ (java.awt.Robot) ghép thành ket_qua_gui.png.
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

    static void inCuaSo(List<DatVeXeGui> ds) throws Exception {
        for (DatVeXeGui cs : ds) log("   " + cs.ten + ": " + ui(cs::thongBaoText));
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

    static boolean dangGiu(DatVeXeGui cs) { return cs.thongBaoText().contains("Đang giữ chỗ"); }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43782;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<DatVeXeGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                DatVeXeGui cs = new DatVeXeGui(ten, host, cong);
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
        DatVeXeGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (chế độ khoa, xe giường nằm, giữ chỗ 30 s)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối");
        lam(() -> { for (DatVeXeGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        ngu(1.0);
        log("2. An, Bình: D01 Sài Gòn → Đà Lạt; Chi: D01 Đà Lạt → Nha Trang; CÙNG bấm 'GIỮ CHỖ'");
        lam(() -> {
            an.datDoan(0, 2);
            binh.datDoan(0, 2);
            chi.datDoan(2, 3);
            for (DatVeXeGui cs : ds) cs.bam("D01");
            for (DatVeXeGui cs : ds) cs.nutGiu.doClick(0);
        });
        cho(() -> ds.stream().noneMatch(DemoGui::dangGiu));
        ngu(0.4);
        inCuaSo(ds);
        inServer(sv);
        List<DatVeXeGui> duoc = ui(() -> ds.stream().filter(cs -> !cs.giu.isEmpty()).toList());
        log("3. " + String.join(" và ", duoc.stream().map(cs -> cs.ten).toList()) + " bấm 'THANH TOÁN'");
        lam(() -> { for (DatVeXeGui cs : duoc) cs.nutTt.doClick(0); });
        cho(() -> duoc.stream().allMatch(cs -> !cs.ve.isEmpty()));
        ngu(0.4);
        for (DatVeXeGui cs : duoc) log("   " + cs.ten + ": " + ui(cs::thongBaoText));
        log("   máy chủ: " + sv.lop.thongKeDayDu().get("tom_tat"));
        log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        lam(() -> {
            sv.oTre.setText("20");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khong_khoa");
        });
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do"))));
        ngu(0.5);
        log("   Cả ba chọn D02 Sài Gòn → Nha Trang rồi CÙNG bấm 'GIỮ CHỖ'");
        Map<String, Integer> truoc = new HashMap<>();
        for (DatVeXeGui cs : ds) truoc.put(cs.ten, ui(() -> cs.soGiu));
        lam(() -> {
            for (DatVeXeGui cs : ds) {
                cs.datDoan(0, 3);
                cs.bam("D02");
            }
            for (DatVeXeGui cs : ds) cs.nutGiu.doClick(0);
        });
        cho(() -> ds.stream().noneMatch(DemoGui::dangGiu));
        for (DatVeXeGui cs : ds)
            log("   " + cs.ten + ": máy chủ trả lời " + (ui(() -> cs.soGiu) > truoc.get(cs.ten) ? "ĐÃ GIỮ D02" : "từ chối"));
        ngu(0.6);
        log("   …sau cap_nhat của máy chủ:");
        inCuaSo(ds);
        inServer(sv);
        Map<String, Object> chuD = Chung.map(sv.lop.trangThai().get("chu"));
        List<String> thuoc = new ArrayList<>();
        for (int k = 0; k < MayChu.SO_CHANG; k++) thuoc.add(Chung.py(chuD.get("D02#" + k)));
        log("   D02 thuộc: " + String.join(", ", thuoc));
        DatVeXeGui nanNhan = ui(() -> ds.stream().filter(cs -> cs.thongBaoText().contains("lấy mất")).findFirst().orElse(binh));
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(nanNhan.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng D01, đoạn ngẫu nhiên)");
        lam(() -> {
            sv.oTre.setText("2");
            sv.nutTre.doClick(0);
            sv.chonCheDo("khoa");
        });
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("6. Đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s; An giữ 05, 06 Sài Gòn → Bảo Lộc (không thanh toán)");
        lam(() -> {
            sv.datThamSo(Json.obj("ttl_s", 6, "loai_xe", "ghe"));
            sv.nutDatLai.doClick(0);
        });
        cho(() -> ds.stream().allMatch(cs -> Chung.sod(cs.tt.get("ttl_s")) == 6 && "ghe".equals(cs.tt.get("loai_xe"))
                && Chung.so(Chung.map(cs.tt.get("dem")).get("ban")) == 0));
        ngu(0.8);
        lam(() -> {
            an.datDoan(0, 1);
            an.bam("05");
            an.bam("06");
            an.nutGiu.doClick(0);
        });
        cho(() -> !an.giu.isEmpty());
        ngu(0.5);
        lam(() -> {
            binh.datDoan(0, 2);
            binh.bam("06");
        });
        log("   Bình bấm 06 (Sài Gòn → Đà Lạt): " + ui(binh::thongBaoText));
        cho(() -> an.giu.isEmpty());
        log("   hết hạn – An: " + ui(an::thongBaoText));
        ngu(0.3);
        log("   Bình giữ 05, 06 Sài Gòn → Đà Lạt rồi thanh toán; Chi mua 05 Đà Lạt → Nha Trang");
        lam(() -> {
            binh.bam("05");
            binh.bam("06");
            chi.datDoan(2, 3);
            chi.bam("05");
            binh.nutGiu.doClick(0);
            chi.nutGiu.doClick(0);
        });
        cho(() -> !binh.giu.isEmpty() && !chi.giu.isEmpty());
        ngu(0.4);
        lam(() -> {
            binh.nutTt.doClick(0);
            chi.nutTt.doClick(0);
        });
        cho(() -> !binh.ve.isEmpty() && !chi.ve.isEmpty());
        ngu(0.4);
        log("   An giữ 10, 11 Sài Gòn → Nha Trang; Chi chọn 21");
        lam(() -> {
            an.datDoan(0, 3);
            an.bam("10");
            an.bam("11");
            an.nutGiu.doClick(0);
            chi.datDoan(0, 3);
            chi.bam("21");
        });
        cho(() -> !an.giu.isEmpty());
        ngu(1.5);
        inCuaSo(ds);
        log("   An: đồng hồ giữ chỗ " + ui(() -> an.nhanDongHo.getText()));
        log("   máy chủ: " + sv.lop.thongKeDayDu().get("tom_tat"));
        inServer(sv);
        log("7. Chụp 4 cửa sổ");
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(an.frame)),
                List.of(GiaoDienChung.chup(binh.frame), GiaoDienChung.chup(chi.frame))), new File(thuMuc, "ket_qua_gui.png"));
        log("8. Máy chủ: STOP SERVER");
        lam(() -> sv.nutStop.doClick(0));
        cho(() -> sv.nutStart.isEnabled());
        ngu(0.5);
        log("   Bình: " + ui(() -> binh.thanhTt.getText()));
        lam(() -> {
            for (DatVeXeGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
