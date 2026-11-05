import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.FontMetrics;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.Insets;
import java.awt.RenderingHints;
import java.awt.image.BufferedImage;
import java.awt.image.DataBufferInt;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.UIManager;
import javax.swing.border.TitledBorder;
import javax.swing.plaf.FontUIResource;

/**
 * Bài mở rộng MR6.5 – Thành phần giao diện Swing dùng chung cho ServerGui, PhatGui, XemGui.
 *
 * Quy tắc (như MR6.2, MR6.3): CHỈ luồng giao diện (EDT) chạm vào điều khiển. Luồng mạng/âm thanh/hiển thị của lõi
 * đưa thông báo vào hàng đợi sự kiện của Swing (SwingUtilities.invokeLater) hoặc ghi số liệu vào đối tượng lõi dưới
 * khóa; bộ hẹn giờ javax.swing.Timer (chạy trên EDT) đọc định kỳ rồi vẽ lại.
 * Ảnh: khung RGB thô 3 byte/điểm được chép vào BufferedImage TYPE_INT_RGB rồi vẽ phóng ×2 kiểu "điểm gần nhất".
 */
final class GiaoDienChung {
    private GiaoDienChung() {}

    static final Color NEN = new Color(0xb0c4de);
    static final Color XANH_LA = new Color(0x90ee90), DO = new Color(0xf08080), XANH_DUONG = new Color(0xadd8e6),
            CAM = new Color(0xffb347), XAM = new Color(0xdddddd);
    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    /** Giao diện Metal (giống nhau trên Windows, Linux) và một cỡ chữ chung. */
    static void caiDat() {
        try { UIManager.setLookAndFeel(UIManager.getCrossPlatformLookAndFeelClassName()); } catch (Exception ignored) { }
        FontUIResource f = new FontUIResource(Font.DIALOG, Font.PLAIN, 12);
        for (Object k : new ArrayList<>(UIManager.getLookAndFeelDefaults().keySet()))
            if (UIManager.get(k) instanceof FontUIResource) UIManager.put(k, f);
    }

    /** Khung nhóm có tiêu đề (như LabelFrame của Tk). */
    static JPanel khungNhom(String tieuDe) {
        JPanel p = new JPanel();
        p.setBackground(NEN);
        TitledBorder b = BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), tieuDe);
        b.setTitleFont(new Font(Font.DIALOG, Font.BOLD, 12));
        p.setBorder(BorderFactory.createCompoundBorder(b, BorderFactory.createEmptyBorder(2, 6, 4, 6)));
        return p;
    }

    static JPanel hang(Component... cs) {
        JPanel p = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 1));
        p.setBackground(NEN);
        for (Component c : cs) p.add(c);
        p.setAlignmentX(0);
        return p;
    }

    static JButton nut(String chu, Color mau, int rong) {
        JButton b = new JButton(chu);
        b.setBackground(mau);
        b.setOpaque(true);
        b.setFocusPainted(false);
        b.setMargin(new Insets(3, 6, 3, 6));
        if (rong > 0) b.setPreferredSize(new Dimension(rong, b.getPreferredSize().height + 2));
        return b;
    }

    static JLabel nhan(String chu) {
        JLabel l = new JLabel(chu);
        l.setAlignmentX(0);
        return l;
    }

    static JLabel nhanDam(String chu) {
        JLabel l = nhan(chu);
        l.setFont(l.getFont().deriveFont(Font.BOLD, 13f));
        return l;
    }

    static JTextField oNhap(Object giaTri, int rong) { return new JTextField(String.valueOf(giaTri), rong); }

    static JLabel thanhTrangThai(String chu) {
        JLabel l = new JLabel(chu);
        l.setOpaque(true);
        l.setBackground(Color.WHITE);
        l.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLoweredBevelBorder(),
                BorderFactory.createEmptyBorder(1, 4, 1, 4)));
        return l;
    }

    static String chuMs(Double x, boolean dau) {
        if (x == null) return "–";
        return dau ? Chung.f("%+.0f ms", x) : Chung.f("%.0f ms", x);
    }

    /** RGB thô 3 byte/điểm -> BufferedImage. */
    static BufferedImage anh(byte[] rgb, int w, int h) {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_RGB);
        int[] px = ((DataBufferInt) img.getRaster().getDataBuffer()).getData();
        for (int i = 0; i < w * h; i++) px[i] = ((rgb[3 * i] & 0xFF) << 16) | ((rgb[3 * i + 1] & 0xFF) << 8) | (rgb[3 * i + 2] & 0xFF);
        return img;
    }

    /** Ô hình nền đen, ảnh phóng theo "điểm gần nhất", chữ giữa ô khi chưa có hình. */
    static final class KhungAnh extends JComponent {
        private BufferedImage anh;
        private String chuGiua = "";

        KhungAnh(int w, int h) { setPreferredSize(new Dimension(w, h)); setMinimumSize(new Dimension(w, h)); }

        void datAnh(BufferedImage a) { anh = a; repaint(); }
        void datChu(String s) { chuGiua = s; repaint(); }

        @Override
        protected void paintComponent(Graphics g0) {
            Graphics2D g = (Graphics2D) g0;
            g.setColor(Color.BLACK);
            g.fillRect(0, 0, getWidth(), getHeight());
            if (anh != null) {
                g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_NEAREST_NEIGHBOR);
                g.drawImage(anh, 0, 0, getWidth(), getHeight(), null);
            } else if (!chuGiua.isEmpty()) {
                g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
                g.setColor(new Color(0xc0c0c0));
                FontMetrics fm = g.getFontMetrics();
                g.drawString(chuGiua, (getWidth() - fm.stringWidth(chuGiua)) / 2, getHeight() / 2 + fm.getAscent() / 2);
            }
        }
    }

    /** Thanh đo mức âm -60 … 0 dBFS (như MR6.2): xanh < -12, vàng < -3, đỏ gần cắt ngọn; chữ dBFS ở bên phải. */
    static final class ThanhMuc extends JComponent {
        static final int CHU = 66;
        private double hien = Double.NEGATIVE_INFINITY, db = Double.NEGATIVE_INFINITY;

        ThanhMuc(int rong) { setPreferredSize(new Dimension(rong, 18)); }

        void dat(double moi) {
            db = moi;
            hien = moi > hien ? moi : hien - 2.0;
            repaint();
        }

        @Override
        protected void paintComponent(Graphics g0) {
            Graphics2D g = (Graphics2D) g0;
            int w = getWidth(), h = getHeight(), rong = w - CHU;
            g.setColor(new Color(0x202020));
            g.fillRect(0, 0, rong, h);
            g.setColor(new Color(0xf4f4f4));
            g.fillRect(rong, 0, CHU, h);
            double tiLe = hien < -60 ? 0 : Math.min(1, (hien + 60) / 60);
            g.setColor(hien < -12 ? new Color(0x30c030) : hien < -3 ? new Color(0xe0c020) : new Color(0xe03030));
            g.fillRect(0, 0, (int) (tiLe * rong), h);
            g.setColor(new Color(0x808080));
            g.drawRect(0, 0, w - 1, h - 1);
            g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
            g.setFont(new Font(Font.DIALOG, Font.PLAIN, 10));
            g.setColor(Color.BLACK);
            String s = db < -90 ? "--- dBFS" : Chung.f("%5.1f dBFS", db);
            FontMetrics fm = g.getFontMetrics();
            g.drawString(s, w - 4 - fm.stringWidth(s), (h + fm.getAscent()) / 2 - 1);
        }
    }

    /** Khi chạy riêng (NhatKy.inRa): cứ `chuKy` giây in một dòng số liệu ra dòng lệnh (không ghi vào ô nhật ký) – để
     *  tệp nhật ký khi thử thiết bị thật cho thấy tiếng/hình có thật sự đi qua. Gọi từ bộ hẹn giờ trên EDT. */
    static final class ThongKeDinhKy {
        final String ten;
        final double chuKy;
        double han;

        ThongKeDinhKy(String ten) { this(ten, 5.0); }
        ThongKeDinhKy(String ten, double chuKy) { this.ten = ten; this.chuKy = chuKy; han = Chung.bayGio() + chuKy; }

        void inNeuDenHan(java.util.function.Supplier<String> taoDong) {
            double bayGio = Chung.bayGio();
            if (!NhatKy.inRa || bayGio < han) return;
            han = bayGio + chuKy;                             // không "đuổi bù" các chu kỳ đã lỡ (lúc chưa phát)
            Chung.nguoiIn("").accept(LocalTime.now().format(GIO) + " [" + ten + "] " + taoDong.get());
        }
    }

    /** Số dBFS để in: "-inf" khi im lặng (như Python). */
    static String db(double x) { return Double.isInfinite(x) ? (x < 0 ? "-inf" : "inf") : Chung.f("%.0f", x); }

    /** Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. Chỉ gọi ghi() trên EDT.
     *  inRa = true (đặt khi chạy riêng từng cửa sổ): mỗi dòng cũng in ra dòng lệnh, kèm tên cửa sổ. */
    static final class NhatKy extends JPanel {
        static volatile boolean inRa;
        final String ten;
        final JTextArea vanBan;

        NhatKy(int cao, int rong, String ten) {
            super(new BorderLayout());
            this.ten = ten;
            vanBan = new JTextArea(cao, rong);
            vanBan.setEditable(false);
            vanBan.setLineWrap(true);
            vanBan.setWrapStyleWord(true);
            vanBan.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 11));
            add(new JScrollPane(vanBan), BorderLayout.CENTER);
        }

        void ghi(String text) {
            String gio = LocalTime.now().format(GIO);
            if (inRa) Chung.nguoiIn("").accept(gio + " [" + ten + "] " + text);
            vanBan.append(gio + " " + text + "\n");
            if (vanBan.getLineCount() > 500) {
                try { vanBan.replaceRange("", 0, vanBan.getLineEndOffset(0)); } catch (javax.swing.text.BadLocationException ignored) { }
            }
            vanBan.setCaretPosition(vanBan.getDocument().getLength());
        }
    }
}
