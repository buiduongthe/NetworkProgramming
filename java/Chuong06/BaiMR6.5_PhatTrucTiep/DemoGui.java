import java.awt.Color;
import java.awt.Graphics2D;
import java.awt.Rectangle;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.JFrame;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.5 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 *
 * Một tiến trình: cửa sổ máy chủ, máy phát P, người xem A và B. P dùng camera giả lập "bảng clapper" và micro giả lập;
 * A vào ngay, đồng bộ tiếng-hình BẬT; B bỏ chọn đồng bộ rồi vào muộn ~3 s. Kịch bản bấm nút như người dùng (mọi thao
 * tác chạy trên EDT qua invokeAndWait, luồng kịch bản chỉ chờ), in số liệu "[demo] …", chụp 4 cửa sổ (java.awt.Robot)
 * ghép thành ket_qua_gui.png. Có ffmpeg: thêm một lượt "Mẫu thử ffmpeg (testsrc)" và một lượt "Tệp video" (tạo tệp
 * mẫu mp4 có chớp + bíp mỗi giây, phát cả hình lẫn tiếng của tệp; chụp máy phát và người xem A thành
 * ket_qua_gui_tep.png).
 *
 * Chạy: xvfb-run -a -s "-screen 0 2000x1400x24" java -cp out DemoGui [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
 */
public class DemoGui {
    static <T> T ui(Callable<T> f) throws Exception {
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> { try { r.set(f.call()); } catch (Exception e) { throw new RuntimeException(e); } });
        return r.get();
    }

    static void lam(Runnable f) throws Exception { SwingUtilities.invokeAndWait(f); }

    static void cho(Callable<Boolean> dk) throws Exception {
        for (int i = 0; i < 300; i++) {                                 // tối đa 15 s
            if (ui(dk)) return;
            Thread.sleep(50);
        }
        log("Kịch bản chờ quá lâu – dừng");
        System.exit(1);
    }

    static void ngu(double giay) throws InterruptedException { Thread.sleep((long) (giay * 1000)); }

    static void log(String s) { Chung.nguoiIn("[demo] ").accept(s); }

    static String ms(Double x, boolean dau) { return x == null ? "–" : dau ? Chung.f("%+.0f", x) : Chung.f("%.0f", x); }

    static void inSoLieu(ServerGui sv, PhatGui p, Map<String, XemGui> ds) {
        MayChu.TrangThai t = sv.lop.trangThai();
        log(Chung.f("   server: vào %.0f, ra %.0f kbit/s, hệ số %.2f", t.kbpsVao, t.kbpsRa, t.heSo));
        for (MayChu.TomTat x : t.nguoiXem)
            log(Chung.f("     -> %s: %s, %.0f kbit/s, I đầu %s ms (%s)", x.ten(), x.nhan(), x.kbps(), ms(x.khungDauMs(), false),
                    x.nguonDau().isEmpty() ? "–" : x.nguonDau()));
        MayPhat.TrangThai tp = p.lop.trangThai();
        log(Chung.f("   P: I %d P %d, TB I %.0f B, P %.0f B, PLI %d", tp.soI, tp.soP, tp.tbI, tp.tbP, tp.pliNhan));
        for (Map.Entry<String, XemGui> e : ds.entrySet()) {
            NguoiXem.TrangThai tt = e.getValue().lop.trangThai();
            NguoiXem.GiaiDoan g = tt.giaiDoan.get(tt.giaiDoan.size() - 1);
            Double lech = g.n > 0 ? g.tong / g.n : null;
            log(Chung.f("   %s: đồng bộ %s, khung đầu %s ms (%s)", e.getKey(), tt.dongBo ? "BẬT" : "TẮT", ms(tt.khungDauMs, false), tt.nguonDau));
            log(Chung.f("      lệch TB %s ms, trễ %s ms, mất tiếng %d, đứng %d", ms(lech, true), ms(tt.treTb, false), tt.matAm, tt.dungHinh));
        }
    }

    static BufferedImage chup(JFrame f) throws Exception {
        Rectangle r = ui(() -> {
            f.toFront();
            f.repaint();
            return new Rectangle(f.getLocationOnScreen(), f.getSize());
        });
        Thread.sleep(250);                                              // chờ vẽ xong
        return new Robot().createScreenCapture(r);
    }

    /** Ghép ảnh theo hàng (căn trên), các hàng xếp dọc (căn trái), nền #d0d0d0, viền 4 – như convert +append/-append. */
    static void ghep(List<List<BufferedImage>> hang, String tep) throws Exception {
        int W = 0, H = 0;
        for (List<BufferedImage> h : hang) {
            int w = 0, c = 0;
            for (BufferedImage a : h) { w += a.getWidth(); c = Math.max(c, a.getHeight()); }
            W = Math.max(W, w);
            H += c;
        }
        BufferedImage ra = new BufferedImage(W + 8, H + 8, BufferedImage.TYPE_INT_RGB);
        Graphics2D g = ra.createGraphics();
        g.setColor(new Color(0xd0d0d0));
        g.fillRect(0, 0, ra.getWidth(), ra.getHeight());
        int y = 4;
        for (List<BufferedImage> h : hang) {
            int x = 4, c = 0;
            for (BufferedImage a : h) {
                g.drawImage(a, x, y, null);
                x += a.getWidth();
                c = Math.max(c, a.getHeight());
            }
            y += c;
        }
        g.dispose();
        ImageIO.write(ra, "png", new File(tep));
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 43652;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        String out = args.length > 3 ? args[3] : "ket_qua_gui.png";
        AtomicReference<ServerGui> rs = new AtomicReference<>();
        AtomicReference<PhatGui> rp = new AtomicReference<>();
        Map<String, XemGui> ds = new LinkedHashMap<>();
        lam(() -> {
            GiaoDienChung.caiDat();
            ServerGui s = new ServerGui(host, tcp, udp);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            PhatGui p = new PhatGui("P", host, tcp);
            p.frame.setLocation(s.frame.getWidth() + 20, 0);
            p.frame.setVisible(true);
            int y = Math.max(s.frame.getHeight(), p.frame.getHeight()) + 40, x = 0;
            for (String ten : new String[] {"A", "B"}) {
                XemGui k = new XemGui(ten, host, tcp);
                k.frame.setLocation(x, y);
                k.frame.setVisible(true);
                x += k.frame.getWidth() + 20;
                ds.put(ten, k);
            }
            rs.set(s);
            rp.set(p);
        });
        ServerGui sv = rs.get();
        PhatGui p = rp.get();
        XemGui a = ds.get("A"), b = ds.get("B");
        ngu(0.5);
        log("1. Server: bấm START SERVER");
        lam(() -> sv.btnStart.doClick());
        log("2. P: Kết nối; camera giả lập (clapper), micro giả lập");
        lam(() -> p.btnKetNoi.doClick());
        cho(() -> p.btnPhat.isEnabled());
        log("   P bấm PHÁT (10 fps, GOP 20)");
        lam(() -> p.btnPhat.doClick());
        ngu(1.0);
        log("3. A: Kết nối ngay (đồng bộ tiếng-hình BẬT)");
        lam(() -> a.btnKetNoi.doClick());
        ngu(3.5);
        log("4. B: bỏ chọn 'Đồng bộ', Kết nối (vào muộn 3,5 s)");
        lam(() -> {
            b.chkDongBo.setSelected(false);
            b.btnKetNoi.doClick();
        });
        cho(() -> b.lop.trangThai().khungDauMs != null);
        ngu(3.0);
        log("5. Số liệu (bộ đệm GOP bật):");
        inSoLieu(sv, p, ds);
        log("6. P bấm 'Khung khóa ngay'; A bấm 'Chụp'");
        lam(() -> p.btnI.doClick());
        ngu(0.6);
        String duong = ui(a::chup);
        log("   A đã chụp " + duong);
        ngu(0.5);
        log("7. Chụp 4 cửa sổ");
        BufferedImage aS = chup(sv.frame), aP = chup(p.frame), aA = chup(a.frame), aB = chup(b.frame);
        ghep(List.of(List.of(aS, aP), List.of(aA, aB)), out);
        log("   Đã ghi " + out + " (" + new File(out).length() / 1024 + " KB)");
        log("8. Server: chọn B, bấm 'Ngắt người chọn'");
        lam(() -> {
            sv.chonNguoiXem("B");
            sv.btnNgat.doClick();
        });
        ngu(0.8);
        log("   B: " + ui(() -> b.thanhTt.getText()));
        if (Camera.coFfmpeg()) {
            log("9. P: DỪNG PHÁT, chọn 'Mẫu thử ffmpeg (testsrc)', PHÁT");
            lam(() -> p.btnDung.doClick());
            ngu(0.5);
            lam(() -> {
                for (int i = 0; i < p.dsCam.size(); i++) if (p.dsCam.get(i)[0].equals(Camera.MAU_THU)) p.cboCam.setSelectedIndex(i);
                p.btnPhat.doClick();
            });
            ngu(4.0);
            MayPhat.TrangThai tp = p.lop.trangThai();
            log(Chung.f("   P testsrc: I %d P %d, TB I %.0f B, P %.0f B", tp.soI, tp.soP, tp.tbI, tp.tbP));
            log("   P: " + tp.camera);
            NguoiXem.TrangThai ta = a.lop.trangThai();
            log(Chung.f("   A: khung I %d, P %d, lệch %s ms", ta.khungI, ta.khungP, ms(ta.lechHien, true)));
            log("10. P: DỪNG PHÁT, tạo tệp mẫu mp4, nguồn Tệp video, PHÁT");
            lam(() -> p.btnDung.doClick());
            new File("out").mkdirs();
            String tep = TepVideo.taoTepMau(new File("out", "tep_mau.mp4").getPath(), 10);
            lam(() -> p.datTep(tep));
            log("   " + ui(() -> p.cboCam.getSelectedItem() + " | " + p.cboMic.getSelectedItem()));
            ngu(0.3);
            lam(() -> p.btnPhat.doClick());
            ngu(4.4);
            tp = p.lop.trangThai();
            log(Chung.f("   P tệp: I %d P %d, TB I %.0f B, P %.0f B", tp.soI, tp.soP, tp.tbI, tp.tbP));
            log("   P: " + (tp.camera.length() > 52 ? tp.camera.substring(0, 52) : tp.camera));
            ta = a.lop.trangThai();
            log(Chung.f("   A: khung I %d, P %d, lệch %s ms, mức tiếng %.1f dBFS", ta.khungI, ta.khungP,
                    ms(ta.lechHien, true), ta.mucAm));
            String ra = new File(new File(out).getAbsoluteFile().getParentFile(), "ket_qua_gui_tep.png").getPath();
            String tenRa = new File(out).getParent() == null ? "./ket_qua_gui_tep.png" : ra;
            ghep(List.of(List.of(chup(p.frame), chup(a.frame))), ra);
            log("   Đã ghi " + tenRa + " (" + new File(ra).length() / 1024 + " KB)");
        }
        log("11. Server bấm STOP SERVER");
        lam(() -> sv.btnStop.doClick());
        cho(() -> sv.btnStart.isEnabled());
        ngu(0.6);
        String tP = ui(() -> p.thanhTt.getText()), tA = ui(() -> a.thanhTt.getText());
        log("   P: " + (tP.length() > 46 ? tP.substring(0, 46) : tP));
        log("   A: " + (tA.length() > 46 ? tA.substring(0, 46) : tA));
        lam(() -> {
            p.dong();
            a.dong();
            b.dong();
            sv.dong();
        });
        log("Kết thúc");
        System.exit(0);
    }
}
