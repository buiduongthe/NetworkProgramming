import java.awt.Color;
import java.awt.Graphics2D;
import java.awt.Rectangle;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.util.Locale;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.JFrame;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.3 – Kịch bản minh họa tự động cho bản giao diện (xvfb-run trên Linux, hoặc chạy thẳng trên Windows).
 * Trong một tiến trình: cửa sổ Camera Server và ba Camera Client A, B (camera giả lập), C (mẫu thử ffmpeg nếu máy có
 * ffmpeg – kiểm tra đường đọc khung thô từ tiến trình ffmpeg). Bấm START SERVER, Kết nối, START; sau đó server
 * giới hạn B 2 fps, đặt C độ phân giải 1/4, tắt camera A; A bấm CANCEL rồi START và bị từ chối; B chọn nhận
 * 5 fps, 1/2. In fps, kbit/s đo được và chụp các cửa sổ (java.awt.Robot) ghép thành ket_qua_gui.png.
 *
 * Chạy: xvfb-run -a -s "-screen 0 1920x1200x24" java -cp out DemoGui [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
 */
public class DemoGui {
    static <T> T ui(Callable<T> f) throws Exception {
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> { try { r.set(f.call()); } catch (Exception e) { throw new RuntimeException(e); } });
        return r.get();
    }

    static void lam(Runnable f) throws Exception { SwingUtilities.invokeAndWait(f); }

    static void cho(String viec, Callable<Boolean> dk) throws Exception {
        for (int i = 0; i < 200; i++) {
            if (ui(dk)) return;
            Thread.sleep(50);
        }
        throw new IllegalStateException("Kịch bản chờ quá lâu: " + viec);
    }

    static void log(String s) { System.out.println("[demo] " + s); }

    static BufferedImage chup(JFrame f) throws Exception {
        Rectangle r = ui(() -> new Rectangle(f.getLocationOnScreen(), f.getSize()));
        return new Robot().createScreenCapture(r);
    }

    static void inSoLieu(String tieuDe, ServerVideoGui sv, KhachVideoGui[] kh) throws Exception {
        log(tieuDe + ":");
        for (int i = 0; i < 3; i++) {
            int j = i;
            log("  server ô " + (i + 1) + ": " + ui(() -> sv.mdlKhach.get(j)));
        }
        for (KhachVideoGui k : kh) {
            PhienVideo p = k.phien;
            log(String.format(Locale.ROOT, "  client %s: gửi %.0f fps %.0f kbit/s (tối đa %d fps, 1/%d) | nhận %.0f fps %.0f kbit/s, lưới %s",
                    p.ten, p.fpsGui, p.kbpsGui, p.fpsServer, p.chia, p.fpsNhan, p.kbpsNhan,
                    p.luoiMoi == null ? "—" : p.luoiMoi.w() + "x" + p.luoiMoi.h()));
        }
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 43637;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        String tepAnh = args.length > 3 ? args[3] : "ket_qua_gui.png";
        String[] ten = {"A", "B", "C"};
        AtomicReference<ServerVideoGui> rs = new AtomicReference<>();
        KhachVideoGui[] kh = new KhachVideoGui[3];
        lam(() -> {
            ServerVideoGui.caiDatGiaoDien();
            ServerVideoGui s = new ServerVideoGui(host, tcp, udp);
            s.frame.setDefaultCloseOperation(JFrame.DISPOSE_ON_CLOSE);
            s.frame.setLocation(0, 0);
            s.frame.setVisible(true);
            rs.set(s);
            for (int i = 0; i < 3; i++) {
                kh[i] = new KhachVideoGui(host, tcp, ten[i]);
                kh[i].frame.setDefaultCloseOperation(JFrame.DISPOSE_ON_CLOSE);
                kh[i].frame.setLocation(i * (kh[i].frame.getWidth() + 8), s.frame.getHeight() + 10);
                kh[i].frame.setVisible(true);
            }
        });
        ServerVideoGui sv = rs.get();
        cho("liệt kê camera", () -> kh[2].cboCamera.getItemCount() > 0 && kh[2].cboCamera.getItemAt(kh[2].cboCamera.getItemCount() - 1).equals(CameraFfmpeg.GIA_LAP));
        Thread.sleep(500);
        log("Danh sách camera trên máy này: " + ui(() -> {
            StringBuilder b = new StringBuilder();
            for (int i = 0; i < kh[2].cboCamera.getItemCount(); i++) b.append(i > 0 ? " | " : "").append(kh[2].cboCamera.getItemAt(i));
            return b.toString();
        }));

        lam(() -> sv.btnStart.doClick());
        cho("server chạy", () -> sv.lblTrangThai.getText().contains("đang chạy"));
        log("Bấm START SERVER -> " + ui(() -> sv.lblTrangThai.getText()));
        for (KhachVideoGui k : kh) {
            lam(() -> k.btnKetNoi.doClick());
            cho("kết nối", () -> k.lblTrangThai.getText().contains("đã kết nối"));
            log("Client " + ui(() -> k.txtTen.getText()) + " bấm Kết nối -> " + ui(() -> k.lblTrangThai.getText()));
        }
        boolean coFfmpeg = CameraFfmpeg.coFfmpeg();
        for (int i = 0; i < 3; i++) {
            KhachVideoGui k = kh[i];
            String chon = i == 2 && coFfmpeg ? CameraFfmpeg.MAU_THU : CameraFfmpeg.GIA_LAP;
            lam(() -> {
                k.cboCamera.setSelectedItem(chon);
                k.btnStart.doClick();
            });
            cho("đang truyền", () -> k.lblTrangThai.getText().contains("Đang truyền"));
            log("Client " + ten[i] + " chọn '" + chon + "', bấm START -> " + ui(() -> k.lblThietBi.getText()));
        }
        Thread.sleep(2500);
        inSoLieu("Lần đo 1 (3 camera bật, mặc định 10 fps, 1/1)", sv, kh);

        // Server: chọn B -> fps tối đa 2; chọn C -> 1/4; chọn A -> Tắt camera
        lam(() -> sv.lstKhach.setSelectedIndex(1));
        lam(() -> sv.spnFps.setValue(2));
        Thread.sleep(200);
        lam(() -> sv.lstKhach.setSelectedIndex(2));
        Thread.sleep(150);
        lam(() -> sv.cboChia.setSelectedIndex(2));
        Thread.sleep(200);
        lam(() -> sv.lstKhach.setSelectedIndex(0));
        Thread.sleep(150);
        lam(() -> sv.btnCamera.doClick());
        cho("A bị tắt", () -> kh[0].lblTrangThai.getText().contains("server đang tắt"));
        log("Server chọn B, đặt fps gửi tối đa 2; chọn C, độ phân giải 1/4; chọn A, bấm Tắt camera -> A: "
                + ui(() -> kh[0].lblTrangThai.getText()));
        lam(() -> kh[0].btnCancel.doClick());
        Thread.sleep(300);
        lam(() -> kh[0].btnStart.doClick());
        cho("A bị từ chối", () -> kh[0].lblTrangThai.getText().contains("từ chối"));
        log("A bấm CANCEL rồi START -> " + ui(() -> kh[0].lblTrangThai.getText()));
        lam(() -> {
            kh[1].cboFpsNhan.setSelectedItem(5);
            kh[1].cboChiaNhan.setSelectedIndex(1);
        });
        log("B chọn nhận 5 fps, độ phân giải 1/2");
        Thread.sleep(3000);
        inSoLieu("Lần đo 2 (A bị server tắt, B tối đa 2 fps, C 1/4, B nhận 5 fps 1/2)", sv, kh);
        lam(() -> sv.lstKhach.setSelectedIndex(2));                // chọn C để thấy điều khiển của C
        Thread.sleep(300);

        BufferedImage anhS = chup(sv.frame);
        BufferedImage[] anhK = new BufferedImage[3];
        for (int i = 0; i < 3; i++) anhK[i] = chup(kh[i].frame);
        int rongK = 8, caoK = 0;
        for (BufferedImage a : anhK) {
            rongK += a.getWidth() + 8;
            caoK = Math.max(caoK, a.getHeight());
        }
        int W = Math.max(anhS.getWidth() + 16, rongK), H = anhS.getHeight() + caoK + 24;
        BufferedImage ghep = new BufferedImage(W, H, BufferedImage.TYPE_INT_RGB);
        Graphics2D g = ghep.createGraphics();
        g.setColor(new Color(0xF5, 0xF5, 0xF5));
        g.fillRect(0, 0, W, H);
        g.drawImage(anhS, 8, 8, null);
        int x = 8;
        for (BufferedImage a : anhK) {
            g.drawImage(a, x, anhS.getHeight() + 16, null);
            x += a.getWidth() + 8;
        }
        g.dispose();
        ImageIO.write(ghep, "png", new File(tepAnh));
        log("Đã chụp cửa sổ server và 3 client: " + tepAnh + " (" + W + "x" + H + ", " + new File(tepAnh).length() / 1024 + " KB)");

        for (KhachVideoGui k : kh) lam(() -> k.btnNgat.doClick());
        Thread.sleep(400);
        lam(() -> sv.btnStop.doClick());
        cho("server dừng", () -> sv.lblTrangThai.getText().contains("đã dừng"));
        log("Ngắt kết nối cả ba, bấm STOP SERVER -> " + ui(() -> sv.lblTrangThai.getText()));
        System.exit(0);
    }
}
