import java.awt.Color;
import java.awt.Graphics2D;
import java.awt.Rectangle;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.SwingUtilities;

/**
 * Bài 5.3 – Kịch bản minh họa tự động cho LookupGui (chạy dưới xvfb-run).
 * Thao tác trên cửa sổ thật như người dùng: bấm Kết nối tới cổng không có server (lỗi REFUSED),
 * bấm Kết nối tới đúng cổng, nhập "TCP" và bấm Tra cứu. Trong lúc chờ server trả lời chậm, đếm số lần
 * đồng hồ giao diện được vẽ lại và chụp cửa sổ (java.awt.Robot); có kết quả thì chụp lần nữa.
 * Hai ảnh được ghép thành ket_qua_gui.png.
 * Chạy: xvfb-run -a java -cp out DemoGui <host> <port> <cổng_không_có_server> [tệp_ảnh]
 */
public class DemoGui {
    static LookupGui win;

    static <T> T ui(Callable<T> f) throws Exception {               // đọc, thao tác điều khiển trên EDT
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> { try { r.set(f.call()); } catch (Exception e) { throw new RuntimeException(e); } });
        return r.get();
    }

    static void waitUntil(Callable<Boolean> cond) throws Exception {
        for (int i = 0; i < 200; i++) {
            if (ui(cond)) return;
            Thread.sleep(50);
        }
        throw new IllegalStateException("Kịch bản chờ quá lâu");
    }

    static void log(String text) { System.out.println("[gui] " + text); }

    static BufferedImage snapshot() throws Exception {
        return chupKhung(win.frame);
    }

    /** Chụp khung cửa sổ bằng java.awt.Robot (gọi ngoài EDT). Trên màn hình thật (Windows) cửa sổ có thể nằm một
     *  phần ngoài màn hình (bố cục dựng cho Xvfb 2600×1700): tạm đưa về góc trên trái màn hình, chụp rồi trả về chỗ
     *  cũ. Lấy biến thể độ phân giải gốc của màn hình (HiDPI); dưới Xvfb chỉ có một biến thể nên như trước. */
    static java.awt.image.BufferedImage chupKhung(java.awt.Window f) throws Exception {
        java.awt.Rectangle[] r = new java.awt.Rectangle[1];
        java.awt.Point[] cu = new java.awt.Point[1];
        javax.swing.SwingUtilities.invokeAndWait(() -> {
            java.awt.Rectangle man = f.getGraphicsConfiguration().getBounds();
            cu[0] = f.getLocation();
            if (!man.contains(new java.awt.Rectangle(f.getLocationOnScreen(), f.getSize()))) f.setLocation(man.x, man.y);
            f.toFront();
            f.repaint();
        });
        Thread.sleep(300);                                          // chờ vẽ xong
        javax.swing.SwingUtilities.invokeAndWait(() -> {
            r[0] = new java.awt.Rectangle(f.getLocationOnScreen(), f.getSize());
            if (System.getProperty("os.name", "").startsWith("Windows")) {   // bỏ viền co giãn trong suốt của Windows 10/11
                java.awt.Insets in = f.getInsets();
                int b = Math.max(0, in.left - 1), d = Math.max(0, in.bottom - 1);
                r[0] = new java.awt.Rectangle(r[0].x + b, r[0].y, r[0].width - 2 * b, r[0].height - d);
            }
        });
        java.util.List<java.awt.Image> ds = new java.awt.Robot().createMultiResolutionScreenCapture(r[0]).getResolutionVariants();
        java.awt.Image a = ds.get(ds.size() - 1);
        java.awt.image.BufferedImage kq = new java.awt.image.BufferedImage(a.getWidth(null), a.getHeight(null),
                java.awt.image.BufferedImage.TYPE_INT_RGB);
        java.awt.Graphics2D g = kq.createGraphics();
        g.drawImage(a, 0, 0, null);
        g.dispose();
        if (!cu[0].equals(r[0].getLocation())) javax.swing.SwingUtilities.invokeAndWait(() -> f.setLocation(cu[0]));
        return kq;
    }

    public static void main(String[] args) throws Exception {
        String host = args[0];
        int port = Integer.parseInt(args[1]), bad = Integer.parseInt(args[2]);
        String out = args.length > 3 ? args[3] : "ket_qua_gui.png";
        SwingUtilities.invokeAndWait(() -> { win = new LookupGui(host, port); win.frame.setVisible(true); });
        Thread.sleep(800);
        ui(() -> { win.txtPort.setText(String.valueOf(bad)); win.btnConnect.doClick(); return null; });
        waitUntil(() -> win.btnConnect.isEnabled());
        log("Kết nối cổng " + bad + ": " + ui(() -> win.lblStatus.getText()));
        ui(() -> { win.txtPort.setText(String.valueOf(port)); win.btnConnect.doClick(); return null; });
        waitUntil(() -> win.btnLookup.isEnabled());
        log("Kết nối cổng " + port + ": " + ui(() -> win.lblStatus.getText()));
        int t0 = ui(() -> { win.txtTerm.setText("TCP"); int t = win.ticks; win.btnLookup.doClick(); return t; });
        Thread.sleep(600);
        log("Đang chờ phản hồi: trạng thái '" + ui(() -> win.lblStatus.getText()) + "', nút Tra cứu "
                + (ui(() -> win.btnLookup.isEnabled()) ? "normal" : "disabled"));
        BufferedImage a = snapshot();
        waitUntil(() -> !win.lblStatus.getText().equals("Đang tra cứu..."));
        log("Kết quả: " + ui(() -> win.txtResult.getText()) + " – trạng thái '" + ui(() -> win.lblStatus.getText()) + "'");
        log("Trong lúc chờ, đồng hồ giao diện được vẽ lại " + (ui(() -> win.ticks) - t0) + " lần (không đứng hình)");
        Thread.sleep(150);
        BufferedImage b = snapshot();
        BufferedImage both = new BufferedImage(a.getWidth() + b.getWidth() + 24, Math.max(a.getHeight(), b.getHeight()) + 16,
                BufferedImage.TYPE_INT_RGB);
        Graphics2D g = both.createGraphics();
        g.setColor(new Color(0xF5, 0xF5, 0xF5));
        g.fillRect(0, 0, both.getWidth(), both.getHeight());
        g.drawImage(a, 8, 8, null);
        g.drawImage(b, a.getWidth() + 16, 8, null);
        g.dispose();
        ImageIO.write(both, "png", new File(out));
        log("Đã chụp cửa sổ: " + out);
        ui(() -> { win.txtTerm.setText("XYZ"); win.btnLookup.doClick(); return null; });   // phản hồi 404
        Thread.sleep(300);
        waitUntil(() -> win.btnLookup.isEnabled());
        log("Tra cứu 'XYZ': " + ui(() -> win.txtResult.getText()) + " – trạng thái '" + ui(() -> win.lblStatus.getText()) + "'");
        SwingUtilities.invokeAndWait(() -> win.frame.dispose());
        System.exit(0);
    }
}
