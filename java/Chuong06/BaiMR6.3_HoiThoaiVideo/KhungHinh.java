import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.FontMetrics;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.RenderingHints;
import java.awt.image.BufferedImage;
import java.awt.image.DataBufferInt;
import javax.swing.JComponent;

/**
 * Ô hiển thị một khung hình (camera của mình, lưới nhận được, hoặc ô của một client trên server).
 * Ảnh được phóng theo kiểu "điểm gần nhất" để thấy rõ độ phân giải thật (1/2, 1/4 hiện thành ô vuông);
 * có dòng chữ chú thích, chữ giữa ô khi chưa có hình, chấm đỏ khi camera tắt, viền khi được chọn.
 * Mọi hàm đặt… chỉ được gọi trên luồng giao diện.
 */
final class KhungHinh extends JComponent {
    private BufferedImage anh;
    private String chuTren = "", chuGiua = "", chuDuoi = "";
    private boolean chamDo, chon;

    KhungHinh(int w, int h) {
        setPreferredSize(new Dimension(w, h));
        setMinimumSize(new Dimension(w / 2, h / 2));
    }

    void datAnh(BufferedImage a) { anh = a; repaint(); }

    void datChu(String tren, String giua, String duoi, boolean chamDo) {
        this.chuTren = tren;
        this.chuGiua = giua;
        this.chuDuoi = duoi;
        this.chamDo = chamDo;
        repaint();
    }

    void datChon(boolean c) {
        if (c != chon) {
            chon = c;
            repaint();
        }
    }

    /** Đổi ảnh RGB thô (3 byte/điểm) thành BufferedImage. */
    static BufferedImage sangAnh(byte[] rgb, int w, int h) {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_RGB);
        int[] px = ((DataBufferInt) img.getRaster().getDataBuffer()).getData();
        for (int i = 0; i < w * h; i++) px[i] = ((rgb[3 * i] & 0xFF) << 16) | ((rgb[3 * i + 1] & 0xFF) << 8) | (rgb[3 * i + 2] & 0xFF);
        return img;
    }

    @Override
    protected void paintComponent(Graphics g0) {
        Graphics2D g = (Graphics2D) g0;
        int w = getWidth(), h = getHeight();
        g.setColor(new Color(0xDD, 0xDD, 0xDD));
        g.fillRect(0, 0, w, h);
        if (anh != null) {                                          // giữ tỉ lệ 4:3, căn giữa
            double k = Math.min((double) w / anh.getWidth(), (double) h / anh.getHeight());
            int dw = (int) (anh.getWidth() * k), dh = (int) (anh.getHeight() * k);
            g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_NEAREST_NEIGHBOR);
            g.drawImage(anh, (w - dw) / 2, (h - dh) / 2, dw, dh, null);
        }
        g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
        g.setFont(getFont() != null ? getFont().deriveFont(Font.BOLD, 12f) : new Font(Font.DIALOG, Font.BOLD, 12));
        FontMetrics fm = g.getFontMetrics();
        if (!chuTren.isEmpty()) nhan(g, chuTren, 4, 4, fm);
        if (!chuDuoi.isEmpty()) nhan(g, chuDuoi, 4, h - fm.getHeight() - 6, fm);
        if (!chuGiua.isEmpty()) {
            g.setColor(new Color(0x55, 0x55, 0x55));
            g.drawString(chuGiua, (w - fm.stringWidth(chuGiua)) / 2, h / 2 + fm.getAscent() / 2);
        }
        if (chamDo) {                                               // chấm đỏ: camera đang tắt
            g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
            g.setColor(new Color(220, 30, 30));
            g.fillOval(w - 20, 6, 14, 14);
        }
        g.setColor(chon ? new Color(30, 110, 230) : new Color(0x88, 0x88, 0x88));
        g.drawRect(0, 0, w - 1, h - 1);
        if (chon) g.drawRect(1, 1, w - 3, h - 3);
    }

    private static void nhan(Graphics2D g, String s, int x, int y, FontMetrics fm) {
        g.setColor(new Color(0, 0, 0, 150));                       // nền mờ để chữ đọc được trên mọi hình
        g.fillRect(x, y, fm.stringWidth(s) + 8, fm.getHeight() + 2);
        g.setColor(Color.WHITE);
        g.drawString(s, x + 4, y + fm.getAscent() + 1);
    }
}
