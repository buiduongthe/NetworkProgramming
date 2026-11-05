import java.awt.Color;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.Rectangle;
import java.awt.RenderingHints;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.JFrame;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.2 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux, hoặc trực tiếp
 * trên Windows). Trong một tiến trình: mở cửa sổ server và ba cửa sổ client A (440 Hz), B (620 Hz), C (880 Hz),
 * rồi thao tác như người dùng – bấm Start Server, Connect, Start Mic (giả lập), Start Mic của server (330 Hz),
 * bật Nghe; khóa mic B, đặt độ lợi C = 200 %, A tự đặt âm lượng nghe 50 %; B xin bật lại mic và bị từ chối.
 * Sau vài giây in mức dBFS đo được, phân tích tần số (Goertzel) 1 giây âm thanh A vừa nghe, và chụp các cửa sổ
 * (java.awt.Robot) ghép thành ket_qua_gui.png.
 *
 * Chạy: xvfb-run -a -s "-screen 0 1920x1200x24" java -cp out DemoGui [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
 */
public class DemoGui {
    static <T> T ui(Callable<T> f) throws Exception {               // đọc/thao tác điều khiển trên EDT
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

    static String db(double v) { return ThanhMuc.chu(v); }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 43627;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        String tepAnh = args.length > 3 ? args[3] : "ket_qua_gui.png";
        String[] ten = {"A", "B", "C"};
        double[] tanSo = {440, 620, 880};

        AtomicReference<ServerAmThanhGui> rs = new AtomicReference<>();
        KhachAmThanhGui[] kh = new KhachAmThanhGui[3];
        lam(() -> {
            ServerAmThanhGui.caiDatGiaoDien();
            ServerAmThanhGui s = new ServerAmThanhGui(host, tcp, udp);
            s.frame.setDefaultCloseOperation(JFrame.DISPOSE_ON_CLOSE);
            s.frame.setBounds(0, 0, 1320, 600);
            s.frame.setVisible(true);
            rs.set(s);
            for (int i = 0; i < 3; i++) {
                kh[i] = new KhachAmThanhGui(host, tcp, ten[i], tanSo[i]);
                kh[i].frame.setDefaultCloseOperation(JFrame.DISPOSE_ON_CLOSE);
                kh[i].frame.setLocation(i * (kh[i].frame.getWidth() + 8), 610);
                kh[i].frame.setVisible(true);
            }
        });
        ServerAmThanhGui sv = rs.get();
        Thread.sleep(800);

        // 1. Start Server
        lam(() -> sv.btnStart.doClick());
        cho("server chạy", () -> sv.lblTrangThai.getText().startsWith("Server đang chạy"));
        log("Bấm Start Server -> " + ui(() -> sv.lblTrangThai.getText()));

        // 2. Ba client bấm Connect
        for (KhachAmThanhGui k : kh) {
            lam(() -> k.btnKetNoi.doClick());
            cho("client kết nối", () -> k.lblTrangThai.getText().startsWith("Đã kết nối"));
            log("Client " + ui(() -> k.txtTen.getText()) + " bấm Connect -> " + ui(() -> k.lblTrangThai.getText()));
        }
        cho("server thấy 3 client", () -> sv.dsDong.size() == 3);
        log("Server: " + ui(() -> sv.lblSoClient.getText()));

        // 3. Mỗi client chọn nguồn giả lập, bấm Start Mic, Start Audio
        for (int i = 0; i < 3; i++) {
            KhachAmThanhGui k = kh[i];
            lam(() -> {
                k.cboMic.setSelectedItem(ThietBiAmThanh.tenGiaLap(k.tanSo));
                k.btnMicStart.doClick();
            });
            cho("mic mở", () -> k.lblTrangThai.getText().endsWith("mic: mở"));
            lam(() -> k.btnAudioStart.doClick());
            log("Client " + ten[i] + " chọn '" + ui(() -> (String) k.cboMic.getSelectedItem()) + "', bấm Start Mic, Start Audio -> "
                    + ui(() -> k.lblTrangThai.getText()) + "; " + ui(() -> k.lblThietBi.getText()));
        }

        // 4. Server bật micro (giả lập 330 Hz) và Nghe
        lam(() -> {
            sv.cboMic.setSelectedItem(ThietBiAmThanh.tenGiaLap(ServerAmThanhGui.TAN_SO_SERVER));
            sv.btnMicStart.doClick();
            sv.chkNghe.doClick();
        });
        cho("mic server", () -> sv.lblThietBiMic.getText().startsWith("Đang dùng"));
        Thread.sleep(300);
        log("Server bấm Start Mic, Nghe -> " + ui(() -> sv.lblThietBiMic.getText()) + "; " + ui(() -> sv.lblLoa.getText()));
        Thread.sleep(1500);
        inMuc("Lần đo 1 (mọi mic mở)", sv, kh);

        // 5. Server khóa mic B, đặt độ lợi C = 200 %; A tự đặt âm lượng nghe 50 %
        lam(() -> sv.dong("B").btnKhoa.doClick());
        lam(() -> sv.dong("C").sldLoi.setValue(200));
        lam(() -> kh[0].sldNghe.setValue(50));
        cho("B bị khóa", () -> kh[1].lblTrangThai.getText().endsWith("bị khóa (server)"));
        log("Server bấm Khóa ở hàng B, kéo độ lợi C lên 200 %; A kéo âm lượng nghe xuống 50 %");
        // 6. B bấm Stop Mic rồi Start Mic: đang bị khóa nên server từ chối (quy tắc 2)
        lam(() -> kh[1].btnMicStop.doClick());
        Thread.sleep(300);
        lam(() -> kh[1].btnMicStart.doClick());
        cho("B bị từ chối", () -> kh[1].lblTrangThai.getText().startsWith("Bị từ chối"));
        log("B bấm Stop Mic, Start Mic -> " + ui(() -> kh[1].lblTrangThai.getText()));
        cho("Server Vol C", () -> kh[2].lblServerVol.getText().equals("Server Vol: 200%"));
        Thread.sleep(2000);
        inMuc("Lần đo 2 (B bị khóa, độ lợi C 200 %, A nghe 50 %)", sv, kh);
        for (KhachAmThanhGui k : kh)
            log("  " + ui(() -> k.txtTen.getText()) + ": " + ui(() -> k.lblServerVol.getText()) + "; trạng thái '" + ui(() -> k.lblTrangThai.getText()) + "'");
        for (String t : ten) {
            ServerAmThanhGui.DongKhach d = ui(() -> sv.dong(t));
            log("  hàng server " + t + ": " + ui(() -> d.lblMicK.getText()) + ", nút '" + ui(() -> d.btnKhoa.getText()) + "', "
                    + ui(() -> d.lblLoi.getText()) + ", " + ui(() -> d.lblNgheK.getText()));
        }

        // 7. Phân tích tần số 1 giây âm thanh A vừa nhận
        short[] mau = kh[0].phien.layGanDay();
        log("Phân tích Goertzel 1 s A nghe được (lý thuyết: 330 Hz server -21.1, 440 Hz chính A vắng, 620 Hz B vắng, 880 Hz C -15.1):");
        for (double f : new double[] {330, 440, 620, 880}) {
            double m = AmThanh.mucAmDBFS(mau, 0, mau.length, f);
            log(String.format(Locale.ROOT, "  %4.0f Hz: %s", f, m < -50 ? "vắng (" + db(m) + ")" : db(m)));
        }

        // 8. Chụp cửa sổ: server ở trên, ba client bên dưới
        Thread.sleep(300);
        BufferedImage anhS = chup(sv.frame);
        BufferedImage[] anhK = new BufferedImage[3];
        for (int i = 0; i < 3; i++) anhK[i] = chup(kh[i].frame);
        int rongK = 0, caoK = 0;
        for (BufferedImage a : anhK) {
            rongK += a.getWidth() + 8;
            caoK = Math.max(caoK, a.getHeight());
        }
        int W = Math.max(anhS.getWidth(), rongK) + 16, H = anhS.getHeight() + caoK + 32 + 16;
        BufferedImage ghep = new BufferedImage(W, H, BufferedImage.TYPE_INT_RGB);
        Graphics2D g = ghep.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
        g.setColor(new Color(0xF5, 0xF5, 0xF5));
        g.fillRect(0, 0, W, H);
        g.drawImage(anhS, 8, 8, null);
        g.setColor(Color.DARK_GRAY);
        g.setFont(new Font(Font.DIALOG, Font.BOLD, 14));
        g.drawString("Ba cửa sổ client (A 440 Hz, B 620 Hz, C 880 Hz – nguồn giả lập)", 8, anhS.getHeight() + 30);
        int x = 8;
        for (BufferedImage a : anhK) {
            g.drawImage(a, x, anhS.getHeight() + 40, null);
            x += a.getWidth() + 8;
        }
        g.dispose();
        ImageIO.write(ghep, "png", new File(tepAnh));
        log("Đã chụp cửa sổ server và 3 client: " + tepAnh + " (" + W + "x" + H + ")");

        // 9. Ngắt, dừng
        for (KhachAmThanhGui k : kh) lam(() -> k.btnNgat.doClick());
        Thread.sleep(400);
        lam(() -> sv.btnStop.doClick());
        cho("server dừng", () -> sv.lblTrangThai.getText().equals("Server đã dừng"));
        log("Disconnect cả ba, bấm Stop Server -> " + ui(() -> sv.lblTrangThai.getText()));
        System.exit(0);
    }

    static void inMuc(String tieuDe, ServerAmThanhGui sv, KhachAmThanhGui[] kh) throws Exception {
        log(tieuDe + ":");
        for (KhachAmThanhGui k : kh)
            log("  client " + ui(() -> k.txtTen.getText()) + ": mic gửi " + ui(() -> k.lblMucMic.getText()) + ", nghe " + ui(() -> k.lblMucNhan.getText()));
        List<String> ds = ui(() -> sv.dsDong.values().stream().map(d -> d.ten + " " + d.lblDb.getText()).toList());
        log("  server – mức vào từng client: " + String.join("; ", ds));
        log("  server – micro " + ui(() -> sv.lblMucMic.getText()) + ", nghe (tổng tiếng client) " + ui(() -> sv.lblMucNghe.getText()));
    }
}
