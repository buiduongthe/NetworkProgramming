import java.awt.Color;
import java.awt.Dimension;
import java.awt.Graphics;
import java.util.Locale;
import javax.swing.JComponent;

/**
 * Thanh đo mức âm (VU meter) đơn giản: -60 dBFS (trống) … 0 dBFS (đầy), xanh / vàng (≥ -18) / đỏ (≥ -6).
 * Chỉ được gọi datMuc trên luồng giao diện (EDT); bộ hẹn giờ Swing của cửa sổ gọi khoảng 10 lần/giây.
 */
final class ThanhMuc extends JComponent {
    private double db = Double.NEGATIVE_INFINITY;

    ThanhMuc(int rong) {
        setPreferredSize(new Dimension(rong, 16));
        setMinimumSize(new Dimension(60, 16));
    }

    void datMuc(double db) {
        if (Double.compare(db, this.db) != 0) {
            this.db = db;
            repaint();
        }
    }

    double muc() { return db; }

    /** "-15.1 dBFS" hoặc "--- dBFS" khi im lặng. */
    static String chu(double db) {
        return db < -90 ? "--- dBFS" : String.format(Locale.ROOT, "%.1f dBFS", db);
    }

    @Override
    protected void paintComponent(Graphics g) {
        int w = getWidth(), h = getHeight();
        g.setColor(new Color(40, 44, 52));
        g.fillRect(0, 0, w, h);
        double tiLe = db == Double.NEGATIVE_INFINITY ? 0 : Math.max(0, Math.min(1, (db + 60) / 60));
        int dai = (int) Math.round(tiLe * (w - 2));
        int vangTu = (int) Math.round((42.0 / 60) * (w - 2)), doTu = (int) Math.round((54.0 / 60) * (w - 2));
        g.setColor(new Color(60, 200, 90));
        g.fillRect(1, 1, Math.min(dai, vangTu), h - 2);
        if (dai > vangTu) {
            g.setColor(new Color(235, 200, 50));
            g.fillRect(1 + vangTu, 1, Math.min(dai, doTu) - vangTu, h - 2);
        }
        if (dai > doTu) {
            g.setColor(new Color(230, 70, 60));
            g.fillRect(1 + doTu, 1, dai - doTu, h - 2);
        }
        g.setColor(new Color(90, 96, 110));                  // vạch chia mỗi 10 dB
        for (int i = 1; i < 6; i++) {
            int x = 1 + i * (w - 2) / 6;
            g.drawLine(x, h - 4, x, h - 2);
        }
    }
}
