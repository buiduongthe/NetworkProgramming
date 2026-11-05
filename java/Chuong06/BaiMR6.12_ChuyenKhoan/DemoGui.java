import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.12 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: bảng điều khiển máy chủ và ba ứng dụng ngân hàng An (TK01), Bình (TK02), Chi (TK03). Kịch bản bấm
 * nút như người dùng (mọi thao tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in "[demo] …":
 *   1–2. chế độ khoa (khóa theo thứ tự mã): An→Bình và Bình→An CÙNG LÚC -> cả hai thành công;
 *   3.   bật "khóa ngây thơ" (nguồn rồi đích), trễ 0–50 ms: chuyển chéo cùng lúc -> KHÓA CHẾT, sau 2 s một giao dịch
 *        bị hủy ("khóa chết: chờ TK.. quá 2 s"); chụp ket_qua_gui_khoa_chet.png;
 *   4.   tắt khóa ngây thơ: chuyển chéo lại -> hết khóa chết;
 *   5.   khong_khoa, trễ 0–20 ms: An→Bình, Bình→Chi, Chi→An cùng lúc -> mất cập nhật, TỔNG TIỀN LỆCH (sai phạm);
 *        chụp ket_qua_gui_tranh_chap.png;
 *   6.   So sánh 4 chế độ (500 người × 5 lần chuyển chéo) -> bảng;
 *   7–8. đặt lại; An chuyển cho Chi, Chi thấy "+ Nhận …"; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
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

    static void inCuaSo(List<NganHangGui> ds) throws Exception {
        for (NganHangGui cs : ds) log("   " + cs.ten + ": " + ui(cs::thongBaoText).split("\n")[0]);
    }

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


    static void chuyen(NganHangGui cs, String den, long soTien) {
        cs.chonNguoiNhan(den);
        cs.datTien(soTien);
        cs.nutChuyen.doClick(0);
    }

    /** An→Bình 500.000 đ và Bình→An 300.000 đ trong cùng một lượt xử lý sự kiện (cùng lúc), rồi chờ hai kết quả. */
    static void cheo(NganHangGui an, NganHangGui binh) throws Exception {
        int dem = ui(() -> an.soKq + binh.soKq);
        lam(() -> {
            chuyen(an, "TK02", 500_000);
            chuyen(binh, "TK01", 300_000);
        });
        cho(() -> an.soKq + binh.soKq == dem + 2);
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43762;
        File thuMuc = new File(".").getAbsoluteFile();
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        List<NganHangGui> ds = new ArrayList<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, cong);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            int x = s.frame.getWidth() + 20, y = 0;
            for (String ten : new String[] {"An", "Bình", "Chi"}) {
                NganHangGui cs = new NganHangGui(ten, host, cong);
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
        NganHangGui an = ds.get(0), binh = ds.get(1), chi = ds.get(2);
        ngu(0.5);
        log("1. Máy chủ: START SERVER (khoa, khóa theo thứ tự mã)");
        lam(() -> sv.nutStart.doClick(0));
        ngu(0.3);
        log("   An, Bình, Chi: Kết nối (TK01, TK02, TK03)");
        lam(() -> { for (NganHangGui cs : ds) cs.nutKetNoi.doClick(0); });
        cho(() -> ds.stream().allMatch(cs -> cs.tt != null));
        ngu(1.0);
        log("2. An→Bình 500.000 đ và Bình→An 300.000 đ CÙNG LÚC");
        cheo(an, binh);
        ngu(0.4);
        inCuaSo(ds.subList(0, 2));
        log("3. Bật 'khóa ngây thơ', Đặt lại; trễ 0–50 ms");
        lam(() -> {
            sv.datKhoaNgayTho(true);
            sv.nutDatLai.doClick(0);
        });
        tre(sv, 50);
        cho(() -> Chung.dung(an.tt.get("khoa_ngay_tho")) && Chung.so(an.tt.get("so_gd")) == 0);
        ngu(0.5);
        log("   An→Bình và Bình→An cùng lúc…");
        cheo(an, binh);
        ngu(0.4);
        inCuaSo(ds.subList(0, 2));
        log("   máy chủ: khóa chết phát hiện " + sv.lop.trangThai().get("khoa_chet"));
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame)), List.of(GiaoDienChung.chup(an.frame), GiaoDienChung.chup(binh.frame))),
                new File(thuMuc, "ket_qua_gui_khoa_chet.png"));
        log("4. Tắt 'khóa ngây thơ', Đặt lại; lặp lại chuyển chéo");
        lam(() -> {
            sv.datKhoaNgayTho(false);
            sv.nutDatLai.doClick(0);
        });
        cho(() -> !Chung.dung(an.tt.get("khoa_ngay_tho")) && Chung.so(an.tt.get("so_gd")) == 0);
        ngu(0.5);
        cheo(an, binh);
        ngu(0.4);
        inCuaSo(ds.subList(0, 2));
        log("5. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại");
        tre(sv, 20);
        lam(() -> sv.chonCheDo("khong_khoa"));
        cho(() -> ds.stream().allMatch(cs -> "khong_khoa".equals(cs.tt.get("che_do"))));
        lam(() -> sv.nutDatLai.doClick(0));
        cho(() -> Chung.so(an.tt.get("so_gd")) == 0);
        ngu(0.5);
        log("   An→Bình, Bình→Chi, Chi→An 1.000.000 đ cùng lúc");
        int dem = ui(() -> ds.stream().mapToInt(cs -> cs.soKq).sum());
        lam(() -> {
            String[] den = {"TK02", "TK03", "TK01"};
            for (int i = 0; i < 3; i++) chuyen(ds.get(i), den[i], 1_000_000);
        });
        cho(() -> ds.stream().mapToInt(cs -> cs.soKq).sum() == dem + 3);
        ngu(0.6);
        inCuaSo(ds);
        inServer(sv);
        ghi(List.of(List.of(GiaoDienChung.chup(sv.frame), GiaoDienChung.chup(chi.frame))),
                new File(thuMuc, "ket_qua_gui_tranh_chap.png"));
        log("6. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
        tre(sv, 2);
        lam(() -> sv.chonCheDo("khoa"));
        cho(() -> sv.lop.kho.cheDo.equals("khoa"));
        ngu(0.3);
        lam(() -> sv.nutSoSanh.doClick(0));
        ngu(0.3);
        cho(() -> !sv.dangMoPhong);
        for (String d : Chung.bangSoSanh(sv.hangSoSanh)) log(d);
        for (String cau : ui(sv::ketLuan).split("\\. ")) log("   " + cau.replaceAll("\\.+$", ""));
        ngu(0.5);
        log("7. Đặt lại; An chuyển 200.000 đ cho Chi");
        lam(() -> sv.nutDatLai.doClick(0));
        cho(() -> Chung.so(an.tt.get("so_gd")) == 0);
        ngu(0.5);
        int dem2 = ui(() -> an.soKq);
        lam(() -> chuyen(an, "TK03", 200_000));
        cho(() -> an.soKq == dem2 + 1);
        ngu(0.8);
        inCuaSo(List.of(an));
        log("   Chi: " + ui(chi::nhanDenText));
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
            for (NganHangGui cs : ds) cs.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
