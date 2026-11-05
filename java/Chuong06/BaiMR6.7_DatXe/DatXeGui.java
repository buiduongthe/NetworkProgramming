import java.awt.BasicStroke;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.RenderingHints;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.awt.geom.AffineTransform;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Path2D;
import java.awt.geom.Rectangle2D;
import java.awt.image.BufferedImage;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/**
 * Bài mở rộng MR6.7 – Ứng dụng gọi xe (người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ ứng
 * dụng gọi xe: bản đồ phố (Graphics2D: đường, công viên, hồ, tên phố), điểm đón (xanh) và điểm đến (đỏ) – bấm chuột
 * trái/phải lên bản đồ để chọn ngã tư –, tuyến đường, các xe (màu theo xe, xe đã có khách mờ đi), giá cước dự kiến, nút
 * ĐẶT XE, trạng thái "Đang tìm tài xế…" / "Đặt thành công – tài xế …" / "Hết chỗ" / "Tài xế … đã nhận khách khác!", thẻ
 * tài xế và nút Hủy chuyến. Đặt được thì xe chạy hoạt hình tới điểm đón rồi tới điểm đến. Mọi số liệu về xe đến từ
 * cap_nhat của máy chủ.
 *
 * Chạy:  java -cp out DatXeGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43712)
 */
public class DatXeGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Map<String, Color> MAU_XE = new HashMap<>();
    static {
        String[][] m = {{"trắng", "f4f4f4"}, {"đỏ", "e53935"}, {"xanh", "1e88e5"}, {"bạc", "b0bec5"}, {"vàng", "fdd835"},
                {"đen", "37474f"}, {"cam", "fb8c00"}, {"tím", "8e24aa"}, {"xám", "78909c"}, {"hồng", "f06292"}};
        for (String[] x : m) MAU_XE.put(x[0], new Color(Integer.parseInt(x[1], 16)));
    }
    static final Color XANH_GRAB = new Color(0x00a651), XANH_TUYEN = new Color(0x1a73e8), DO_PIN = new Color(0xd93025);
    static final double TOC_DO = 0.28;                       // đơn vị bản đồ / giây (hoạt hình)

    /** Đường đi trên lưới phố từ a tới b: đi ngang trước rồi đi dọc. */
    static List<double[]> duongDi(double[] a, double[] b) {
        List<double[]> ds = new ArrayList<>();
        ds.add(a);
        if (a[0] != b[0] && a[1] != b[1]) ds.add(new double[] {b[0], a[1]});
        ds.add(b);
        return ds;
    }

    static double doDai(List<double[]> ds) {
        double s = 0;
        for (int i = 0; i + 1 < ds.size(); i++) s += Math.abs(ds.get(i)[0] - ds.get(i + 1)[0]) + Math.abs(ds.get(i)[1] - ds.get(i + 1)[1]);
        return s;
    }

    /** Điểm cách đầu đường đi một đoạn s (đơn vị bản đồ) và hướng (true = ngang). */
    static Object[] diemTren(List<double[]> ds, double s) {
        for (int i = 0; i + 1 < ds.size(); i++) {
            double[] p = ds.get(i), q = ds.get(i + 1);
            double d = Math.abs(p[0] - q[0]) + Math.abs(p[1] - q[1]);
            if (s <= d || i + 2 == ds.size()) {
                double t = d == 0 ? 0 : Math.min(1.0, s / d);
                return new Object[] {new double[] {p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t}, p[1] == q[1]};
            }
            s -= d;
        }
        return new Object[] {ds.get(ds.size() - 1), true};
    }

    /** Phần đường đi từ khoảng cách s0 tới s1 (danh sách điểm). */
    static List<double[]> doan(List<double[]> ds, double s0, double s1) {
        List<double[]> kq = new ArrayList<>();
        kq.add((double[]) diemTren(ds, s0)[0]);
        double tich = 0;
        for (int i = 0; i + 1 < ds.size(); i++) {
            tich += Math.abs(ds.get(i)[0] - ds.get(i + 1)[0]) + Math.abs(ds.get(i)[1] - ds.get(i + 1)[1]);
            if (s0 < tich && tich < s1) kq.add(ds.get(i + 1));
        }
        kq.add((double[]) diemTren(ds, s1)[0]);
        return kq;
    }

    /** Bản đồ phố (dùng cả ở ứng dụng gọi xe và bảng điều khiển máy chủ). Nền vẽ một lần vào ảnh đệm. */
    static final class BanDo {
        final int w, h;
        final boolean nho;
        private BufferedImage nen;

        BanDo(int w, int h, boolean nho) { this.w = w; this.h = h; this.nho = nho; }

        double px(double x) { return x * w; }

        double py(double y) { return y * h; }

        private Rectangle2D o(double x0, double y0, double x1, double y1) {
            double le = 0.022;
            return new Rectangle2D.Double(px(x0 + le), py(y0 + le), px(x1 - le) - px(x0 + le), py(y1 - le) - py(y0 + le));
        }

        void veNen(Graphics2D g) {
            if (nen == null) {
                nen = new BufferedImage(w, h, BufferedImage.TYPE_INT_RGB);
                Graphics2D b = nen.createGraphics();
                b.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
                b.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
                veNenThat(b);
                b.dispose();
            }
            g.drawImage(nen, 0, 0, null);
        }

        private void veNenThat(Graphics2D g) {
            g.setColor(new Color(0xebe7df));
            g.fillRect(0, 0, w, h);
            double[] bien = new double[MayChu.DUONG.length + 2];
            bien[0] = 0;
            System.arraycopy(MayChu.DUONG, 0, bien, 1, MayChu.DUONG.length);
            bien[bien.length - 1] = 1;
            for (int i = 0; i + 1 < bien.length; i++)                            // khu phố: nhà (ô nhỏ)
                for (int j = 0; j + 1 < bien.length; j++) {
                    Rectangle2D r = o(bien[i], bien[j], bien[i + 1], bien[j + 1]);
                    if (r.getWidth() < 6 || r.getHeight() < 6) continue;
                    g.setColor(new Color(0xe2ddd2));
                    g.fill(r);
                    if (!nho && (i + j) % 2 == 0) {
                        g.setColor(new Color(0xd6d0c2));
                        g.fill(new Rectangle2D.Double(r.getX() + 4, r.getY() + 4, r.getWidth() / 2 - 6, r.getHeight() / 2 - 6));
                    }
                }
            Rectangle2D cv = o(0.3, 0.7, 0.5, 0.9);                              // công viên
            g.setColor(new Color(0xbfe3a6));
            g.fill(cv);
            for (int k = 0; k < 5; k++) {
                double x = cv.getX() + cv.getWidth() * (0.2 + 0.15 * k), y = cv.getY() + cv.getHeight() * (k % 2 == 1 ? 0.35 : 0.65);
                double r = nho ? 3 : 6;
                g.setColor(new Color(0x7cbf5a));
                g.fill(new Ellipse2D.Double(x - r, y - r, 2 * r, 2 * r));
            }
            g.setFont(new Font(Font.DIALOG, Font.PLAIN, 9));
            if (!nho) {
                g.setColor(new Color(0x3d7a2a));
                GiaoDienChung.chuGiua(g, "Công viên", cv.getCenterX(), cv.getMaxY() - 7);
            }
            Rectangle2D ho = o(0.7, 0.1, 0.9, 0.3);                              // hồ
            Ellipse2D e = new Ellipse2D.Double(ho.getX(), ho.getY(), ho.getWidth(), ho.getHeight());
            g.setColor(new Color(0xa9d3f2));
            g.fill(e);
            g.setColor(new Color(0x8cc2ea));
            g.draw(e);
            if (!nho) {
                g.setColor(new Color(0x2a6fa8));
                GiaoDienChung.chuGiua(g, "Hồ", ho.getCenterX(), ho.getCenterY());
            }
            int rong = nho ? 7 : 12;
            for (double x : MayChu.DUONG) {                                      // đường dọc và ngang
                boolean chinh = x == 0.5;
                g.setColor(chinh ? new Color(0xfde293) : Color.WHITE);
                g.setStroke(new BasicStroke(rong + (chinh ? 2 : 0)));
                g.draw(new java.awt.geom.Line2D.Double(px(x), 0, px(x), h));
                g.draw(new java.awt.geom.Line2D.Double(0, py(x), w, py(x)));
            }
            if (!nho) {
                g.setColor(new Color(0x7a7468));
                for (int i = 0; i < MayChu.DUONG.length; i++) {                  // tên phố: giữa hai ngã tư
                    AffineTransform cu = g.getTransform();
                    g.translate(MayChu.DUONG[i] * w + 10, (i % 2 == 1 ? 0.4 : 0.6) * h);
                    g.rotate(Math.PI / 2);
                    GiaoDienChung.chuGiua(g, MayChu.TEN_DOC[i], 0, 0);
                    g.setTransform(cu);
                    GiaoDienChung.chuGiua(g, MayChu.TEN_NGANG[i], (i % 2 == 1 ? 0.4 : 0.6) * w, MayChu.DUONG[i] * h - 10);
                }
            }
        }

        void tuyen(Graphics2D g, List<double[]> ds, Color mau, boolean net) {
            if (ds.size() < 2) return;
            Path2D.Double p = new Path2D.Double();
            p.moveTo(px(ds.get(0)[0]), py(ds.get(0)[1]));
            for (int i = 1; i < ds.size(); i++) p.lineTo(px(ds.get(i)[0]), py(ds.get(i)[1]));
            int rong = nho ? 3 : 5;
            if (!net) {
                g.setColor(new Color(0x0b4fb3));
                g.setStroke(new BasicStroke(rong + 2, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
                g.draw(p);
            }
            g.setColor(mau);
            g.setStroke(net ? new BasicStroke(rong, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND, 1, new float[] {8, 6}, 0)
                    : new BasicStroke(rong, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
            g.draw(p);
        }

        void ghim(Graphics2D g, double[] d, Color mau, String chu) {
            double x = px(d[0]), y = py(d[1]), r = nho ? 5 : 9;
            Path2D.Double t = new Path2D.Double();
            t.moveTo(x - r * 0.8, y - r * 1.4);
            t.lineTo(x + r * 0.8, y - r * 1.4);
            t.lineTo(x, y);
            t.closePath();
            g.setColor(mau);
            g.fill(t);
            Ellipse2D e = new Ellipse2D.Double(x - r, y - r * 2.6, 2 * r, 2 * r);
            g.fill(e);
            g.setColor(Color.WHITE);
            g.setStroke(new BasicStroke(2));
            g.draw(e);
            g.fill(new Ellipse2D.Double(x - r * 0.35, y - r * 1.95, r * 0.7, r * 0.7));
            if (chu != null && !nho) {
                g.setColor(mau);
                g.setFont(new Font(Font.DIALOG, Font.BOLD, 11));
                var fm = g.getFontMetrics();
                g.drawString(chu, (float) (x + r + 3), (float) (y - r * 1.6 + fm.getAscent() / 2.0 - 1));
            }
        }

        void xe(Graphics2D g, double[] d, Color mau, String nhan, boolean ngang, boolean mo, boolean noiBat) {
            double x = px(d[0]), y = py(d[1]);
            double dai = nho ? 14 : 24, ngan = nho ? 8 : 13;
            double rx = ngang ? dai / 2 : ngan / 2, ry = ngang ? ngan / 2 : dai / 2;
            if (noiBat) {
                double r = dai * 0.85;
                g.setColor(XANH_GRAB);
                g.setStroke(new BasicStroke(3));
                g.draw(new Ellipse2D.Double(x - r, y - r, 2 * r, 2 * r));
            }
            g.setStroke(new BasicStroke(1));
            g.setColor(new Color(0x9a9488));
            g.fill(new Rectangle2D.Double(x - rx + 1, y - ry + 2, 2 * rx, 2 * ry));
            Rectangle2D than = new Rectangle2D.Double(x - rx, y - ry, 2 * rx, 2 * ry);
            g.setColor(mo ? new Color(0xd0d0d0) : mau);
            g.fill(than);
            g.setColor(mo ? new Color(0x555555) : new Color(0x202020));
            g.draw(than);
            g.setColor(new Color(0x5c6f80));                                    // kính lái, kính sau
            if (ngang) {
                g.fill(new Rectangle2D.Double(x + rx * 0.25, y - ry + 2, rx * 0.3, 2 * ry - 4));
                g.fill(new Rectangle2D.Double(x - rx * 0.7, y - ry + 2, rx * 0.25, 2 * ry - 4));
            } else {
                g.fill(new Rectangle2D.Double(x - rx + 2, y + ry * 0.25, 2 * rx - 4, ry * 0.3));
                g.fill(new Rectangle2D.Double(x - rx + 2, y - ry * 0.7, 2 * rx - 4, ry * 0.25));
            }
            if (nhan != null && !nhan.isEmpty()) {
                g.setColor(mo ? new Color(0x707070) : new Color(0x202020));
                g.setFont(new Font(Font.DIALOG, Font.BOLD, nho ? 9 : 11));
                GiaoDienChung.chuGiua(g, nhan, x, y - Math.max(rx, ry) - 6);
            }
        }
    }

    static Color mauXe(Object ten) { return MAU_XE.getOrDefault(String.valueOf(ten), new Color(0xf4f4f4)); }

    // ------------------------------------------------------------ trạng thái cửa sổ
    static final int RONG = 440, CAO = 340;
    double[] don, den;
    String trang;                 // san_sang | dang_tim | da_dat | dang_huy | het_cho | mat_xe | xong
    Long xeToi;                   // mã xe được báo đặt thành công
    Map<String, Object> chuyen;   // thông tin chuyến từ kq
    long phienToi;                // số thứ tự lần ghi (kq) của chuyến mình đặt
    HoatHinh hoatHinh;
    String pha;
    int soDat, soDuoc;
    double tTim;
    BanDo banDo;
    JComponent cv, cvTx;
    JLabel nhanDon, nhanDen, nhanCuoc, nhanTt, nhanTx, nhanTx2, nhanXeTrong, nhanCuaToi;
    JButton nutDat, nutHuy;
    Timer henHh;
    String chuTx;

    record HoatHinh(List<double[]> ds, double t0, double dai, double moc) {}

    DatXeGui(String ten, String host, int cong) {
        super("Gọi xe", ten, host, cong);
        xay();
    }

    static JLabel dong(String chu, Font f, Color mau, Color nen) {
        JLabel l = GiaoDienChung.nhan(chu, f, mau);
        l.setOpaque(nen != null);
        if (nen != null) l.setBackground(nen);
        l.setMaximumSize(new Dimension(Integer.MAX_VALUE, l.getPreferredSize().height + 20));
        return l;
    }

    @Override
    protected void ve(JPanel khung) {
        double[][] d = DatXe.diemMacDinh(ten);
        don = d[0];
        den = d[1];
        trang = "san_sang";
        khung.setLayout(new BoxLayout(khung, BoxLayout.X_AXIS));
        JPanel trai = GiaoDienChung.cot();
        banDo = new BanDo(RONG, CAO, false);
        cv = new GiaoDienChung.KhungVe(RONG, CAO, this::veDong);
        cv.setBorder(BorderFactory.createLineBorder(new Color(0x808080)));
        cv.addMouseListener(new MouseAdapter() {
            @Override public void mousePressed(MouseEvent e) { chon(e.getX(), e.getY(), SwingUtilities.isRightMouseButton(e) ? "den" : "don"); }
        });
        trai.add(Box.createVerticalStrut(4));
        trai.add(cv);
        trai.add(GiaoDienChung.nhan("Chuột trái: chọn điểm đón · chuột phải: chọn điểm đến", GiaoDienChung.chu(11, false), new Color(0x404040)));
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(8));

        JPanel phai = new JPanel();
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        phai.setBackground(Color.WHITE);
        phai.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLineBorder(Color.BLACK),
                BorderFactory.createEmptyBorder(6, 8, 6, 8)));
        phai.setPreferredSize(new Dimension(276, CAO + 20));
        phai.setAlignmentY(0);
        phai.add(dong("Bạn muốn đi đâu?", GiaoDienChung.chu(16, true), Color.BLACK, null));
        nhanDon = dong(" ", GiaoDienChung.chu(12, false), XANH_GRAB, null);
        nhanDen = dong(" ", GiaoDienChung.chu(12, false), DO_PIN, null);
        nhanCuoc = dong(" ", GiaoDienChung.chu(13, true), Color.BLACK, null);
        phai.add(nhanDon);
        phai.add(nhanDen);
        phai.add(Box.createVerticalStrut(4));
        phai.add(nhanCuoc);
        phai.add(Box.createVerticalStrut(4));
        nutDat = GiaoDienChung.nut("ĐẶT XE", XANH_GRAB, 0);
        nutDat.setForeground(Color.WHITE);
        nutDat.setFont(GiaoDienChung.chu(16, true));
        nutDat.setBorderPainted(false);
        nutDat.setAlignmentX(0);
        nutDat.setMaximumSize(new Dimension(Integer.MAX_VALUE, 36));
        nutDat.addActionListener(e -> datXe());
        phai.add(nutDat);
        phai.add(Box.createVerticalStrut(4));
        nhanTt = dong("Chưa kết nối", GiaoDienChung.chu(13, true), Color.BLACK, new Color(0xe8e8e8));
        nhanTt.setHorizontalAlignment(JLabel.CENTER);
        nhanTt.setBorder(BorderFactory.createEmptyBorder(5, 4, 5, 4));
        phai.add(nhanTt);
        phai.add(Box.createVerticalStrut(4));
        JPanel the = new JPanel();                                           // thẻ tài xế
        the.setLayout(new BoxLayout(the, BoxLayout.X_AXIS));
        the.setBackground(new Color(0xf5f7f6));
        the.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createEtchedBorder(), BorderFactory.createEmptyBorder(4, 4, 4, 4)));
        the.setAlignmentX(0);
        the.setMaximumSize(new Dimension(Integer.MAX_VALUE, 60));
        cvTx = new GiaoDienChung.KhungVe(44, 44, g -> {
            if (chuTx == null) return;
            g.setColor(XANH_GRAB);
            g.fill(new Ellipse2D.Double(4, 4, 36, 36));
            g.setColor(Color.WHITE);
            g.setFont(GiaoDienChung.chu(20, true));
            GiaoDienChung.chuGiua(g, chuTx, 22, 22);
        });
        the.add(cvTx);
        the.add(Box.createHorizontalStrut(6));
        JPanel f = GiaoDienChung.cot();
        f.setBackground(new Color(0xf5f7f6));
        nhanTx = GiaoDienChung.nhan("Tài xế: –", GiaoDienChung.chu(13, true), null);
        nhanTx2 = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), null);
        f.add(nhanTx);
        f.add(nhanTx2);
        the.add(f);
        the.add(Box.createHorizontalGlue());
        phai.add(the);
        phai.add(Box.createVerticalStrut(4));
        nutHuy = GiaoDienChung.nut("Hủy chuyến", new Color(0xf0f0f0), 150);
        nutHuy.addActionListener(e -> huy());
        phai.add(nutHuy);
        phai.add(Box.createVerticalStrut(4));
        nhanXeTrong = dong(" ", GiaoDienChung.chu(12, false), Color.BLACK, null);
        nhanCuaToi = dong(" ", GiaoDienChung.chu(11, false), new Color(0x505050), null);
        phai.add(nhanXeTrong);
        phai.add(nhanCuaToi);
        phai.add(Box.createVerticalGlue());
        JPanel boc = GiaoDienChung.cot();
        boc.add(Box.createVerticalStrut(4));
        boc.add(phai);
        khung.add(boc);
        veDiem();
        batNut();
        bao("Chưa kết nối", new Color(0xe8e8e8), Color.BLACK);           // nạp sẵn bộ hiển thị HTML của Swing
        henHh = new Timer(40, e -> nhipHoatHinh());
    }

    // ------------------------------------------------------------ chọn điểm
    void chon(int x0, int y0, String loai) {
        if (trang.equals("dang_tim") || trang.equals("da_dat")) return;
        double x = (double) x0 / RONG, y = (double) y0 / CAO;
        double[] p = null;
        double tot = 0;
        for (double[] q : MayChu.NGA_TU) {
            double d = (q[0] - x) * (q[0] - x) + (q[1] - y) * (q[1] - y);
            if (p == null || d < tot) { p = q; tot = d; }
        }
        if (loai.equals("don") && !java.util.Arrays.equals(p, den)) don = p;
        else if (loai.equals("den") && !java.util.Arrays.equals(p, don)) den = p;
        veDiem();
        cv.repaint();
    }

    void veDiem() {
        nhanDon.setText("● Đón:  " + MayChu.tenDiem(don));
        nhanDen.setText("● Đến:  " + MayChu.tenDiem(den));
        nhanCuoc.setText(Chung.f("%.1f km · cước dự kiến ", MayChu.km(don, den)) + Chung.tien(MayChu.cuoc(don, den)));
    }

    boolean coTheDat() { return kn != null && List.of("san_sang", "het_cho", "mat_xe", "xong").contains(trang); }

    void batNut() {
        boolean co = coTheDat();
        nutDat.setEnabled(co);
        nutDat.setBackground(co ? XANH_GRAB : new Color(0x7fcfa5));
        nutHuy.setEnabled(trang.equals("da_dat"));
        nutHuy.setText(hoatHinh != null && daToi() ? "Hoàn tất chuyến" : "Hủy chuyến");
    }

    void bao(String text, Color nen, Color chu) {
        nhanTt.setText(GiaoDienChung.html(text).replace("<html>", "<html><body style='width:214px;text-align:center'>")
                .replace("</html>", "</body></html>"));
        nhanTt.putClientProperty("chu", text);
        nhanTt.setBackground(nen);
        nhanTt.setForeground(chu);
    }

    String chuTt() {
        Object c = nhanTt.getClientProperty("chu");
        return c == null ? nhanTt.getText() : c.toString();
    }

    // ------------------------------------------------------------ đặt / hủy
    void datXe() {
        if (kn == null || trang.equals("dang_tim") || trang.equals("da_dat")) return;
        trang = "dang_tim";
        soDat++;
        tTim = Chung.bayGio();
        guiYc("dat_xe", Json.obj("don", MayChu.dsDiem(don), "den", MayChu.dsDiem(den)));   // gửi trước, vẽ sau
        bao("Đang tìm tài xế…", new Color(0xfff3c4), Color.BLACK);
        nhatKy.ghi("Đặt xe: " + MayChu.tenDiem(don) + " -> " + MayChu.tenDiem(den));
        batNut();
    }

    void huy() {
        if (trang.equals("da_dat")) {
            trang = "dang_huy";
            batNut();
            guiYc("huy", Json.obj());
        }
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        if (loai.equals("dat_xe")) {
            if (Chung.dung(tin.get("ok"))) {
                soDuoc++;
                trang = "da_dat";
                xeToi = Chung.so(tin.get("xe"));
                chuyen = tin;
                phienToi = Chung.so(tin.get("phien"));
                bao("Đặt thành công – tài xế " + tin.get("tai_xe"), new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                nhatKy.ghi("  ĐẶT THÀNH CÔNG: " + tin.get("tai_xe") + " " + tin.get("bien_so") + ", đến sau " + tin.get("phut")
                        + " phút" + Chung.f(" (%.1f ms)", rtt));
                theTaiXe(tin);
                Map<String, Object> xe = xe(xeToi);
                double[] goc = xe != null ? MayChu.diemCua(xe.get("vi_tri")) : don;
                List<double[]> ds = duongDi(goc, don);
                double moc = doDai(ds);
                List<double[]> di = duongDi(don, den);
                ds.addAll(di.subList(1, di.size()));
                hoatHinh = new HoatHinh(ds, Chung.bayGio(), doDai(ds), moc);
                kiemXe(tt);
            } else if ("het_cho".equals(tin.get("ma_loi"))) {
                trang = "het_cho";
                bao("Hết chỗ – không còn xe trống", new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                nhatKy.ghi(Chung.f("  HẾT CHỖ (%.1f ms)", rtt));
            } else {
                trang = "het_cho";
                bao("Không đặt được: " + tin.get("ly_do"), new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                nhatKy.ghi("  từ chối: " + tin.get("ly_do"));
            }
        } else {
            if (Chung.dung(tin.get("ok"))) {
                boolean xong = hoatHinh != null && daToi();
                nhatKy.ghi(xong ? "Hoàn tất chuyến, cảm ơn bạn!" : "Đã hủy chuyến (" + tin.get("tai_xe") + ")");
                bao(xong ? "Hoàn tất chuyến – cảm ơn bạn!" : "Đã hủy chuyến", new Color(0xe8e8e8), Color.BLACK);
                trang = "xong";
            } else {
                nhatKy.ghi("  hủy bị từ chối: " + tin.get("ly_do"));
                trang = "san_sang";
            }
            xeToi = null;
            chuyen = null;
            hoatHinh = null;
            theTaiXe(null);
        }
        nhanCuaToi.setText("Bạn: bấm đặt " + soDat + " lần, được báo thành công " + soDuoc);
        batNut();
        cv.repaint();
    }

    void theTaiXe(Map<String, Object> tin) {
        if (tin == null) {
            chuTx = null;
            nhanTx.setText("Tài xế: –");
            nhanTx2.setText(" ");
        } else {
            String t = String.valueOf(tin.get("tai_xe"));
            chuTx = t.substring(0, 1);
            nhanTx.setText(t + "  ★ 4.9");
            nhanTx2.setText(GiaoDienChung.html(tin.get("bien_so") + " · xe " + tin.get("mau") + "\nđến sau " + tin.get("phut")
                    + " phút · " + Chung.tien(Chung.so(tin.get("cuoc")))));
        }
        cvTx.repaint();
    }

    Map<String, Object> xe(Long ma) {
        if (tt == null || ma == null) return null;
        for (Object o : Chung.list(tt.get("xe"))) if (Chung.so(Chung.map(o).get("ma")) == ma) return Chung.map(o);
        return null;
    }

    /** Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq). Xe mình được báo đặt thành công mà
     *  lại ghi khách khác: bị "cướp" chỗ (tranh chấp); xe trống: máy chủ đặt lại. */
    void kiemXe(Map<String, Object> t) {
        if (!trang.equals("da_dat") || xeToi == null || t == null || Chung.so(t.get("phien")) < phienToi) return;
        Map<String, Object> x = xe(xeToi);
        if (x != null && ten.equals(x.get("khach"))) return;
        if (x != null && x.get("khach") != null) {
            trang = "mat_xe";
            hoatHinh = null;
            bao("Tài xế " + x.get("tai_xe") + " đã nhận khách khác (" + x.get("khach") + ")!", new Color(0xff6060), Color.WHITE);
            nhatKy.ghi("Tài xế " + x.get("tai_xe") + " đã nhận khách khác: " + x.get("khach"));
        } else {
            trang = "san_sang";
            xeToi = null;
            chuyen = null;
            hoatHinh = null;
            theTaiXe(null);
            bao("Chuyến đã kết thúc (máy chủ đặt lại)", new Color(0xe8e8e8), Color.BLACK);
        }
        batNut();
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        nhanXeTrong.setText("Xe trống quanh bạn: " + t.get("trong") + "/" + t.get("so_xe"));
        if (!trang.equals("dang_tim") && chuTt().equals("Chưa kết nối"))
            bao("Chọn điểm đón và bấm ĐẶT XE", new Color(0xe8e8e8), Color.BLACK);
        kiemXe(t);
        batNut();
        cv.repaint();
    }

    boolean daToi() { return (Chung.bayGio() - hoatHinh.t0()) * TOC_DO >= hoatHinh.dai(); }

    void veDong(Graphics2D g) {
        banDo.veNen(g);
        HoatHinh hh = hoatHinh;
        if (hh != null) {
            double s = (Chung.bayGio() - hh.t0()) * TOC_DO;
            if (s < hh.moc()) {                                      // xe đang tới điểm đón: tuyến nét đứt
                banDo.tuyen(g, doan(hh.ds(), s, hh.moc()), new Color(0x5f8fd8), true);
                banDo.tuyen(g, duongDi(don, den), XANH_TUYEN, false);
            } else banDo.tuyen(g, doan(hh.ds(), s, hh.dai()), XANH_TUYEN, false);
        } else banDo.tuyen(g, duongDi(don, den), new Color(0x8fb4ec), false);
        banDo.ghim(g, don, XANH_GRAB, "Đón");
        banDo.ghim(g, den, DO_PIN, "Đến");
        if (tt != null)
            for (Object o : Chung.list(tt.get("xe"))) {
                Map<String, Object> x = Chung.map(o);
                if (xeToi != null && Chung.so(x.get("ma")) == xeToi && hh != null) continue;
                banDo.xe(g, MayChu.diemCua(x.get("vi_tri")), mauXe(x.get("mau")), String.valueOf(x.get("tai_xe")), true,
                        x.get("khach") != null, false);
            }
        if (hh != null && chuyen != null) {
            Object[] d = diemTren(hh.ds(), Math.min(hh.dai(), (Chung.bayGio() - hh.t0()) * TOC_DO));
            banDo.xe(g, (double[]) d[0], mauXe(chuyen.get("mau")), String.valueOf(chuyen.get("tai_xe")), (boolean) d[1], false, true);
        }
    }

    @Override
    protected void moiNhip() {
        if (trang.equals("dang_tim")) {
            String cham = ".".repeat(1 + (int) ((Chung.bayGio() - tTim) * 5) % 3);
            bao("Đang tìm tài xế" + cham, new Color(0xfff3c4), Color.BLACK);
        }
    }

    void nhipHoatHinh() {
        if (hoatHinh != null) {
            double s = (Chung.bayGio() - hoatHinh.t0()) * TOC_DO;
            String p = s < hoatHinh.moc() ? "toi_don" : s < hoatHinh.dai() ? "di" : "den";
            if (!p.equals(pha)) {
                pha = p;
                if (p.equals("di")) {
                    bao("Tài xế " + chuyen.get("tai_xe") + " đã đón bạn – đang đi", new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                    nhatKy.ghi("Tài xế đã tới điểm đón");
                } else if (p.equals("den")) {
                    bao("Đã tới nơi – bấm Hoàn tất chuyến", new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                    nhatKy.ghi("Đã tới " + MayChu.tenDiem(den));
                    batNut();
                }
            }
            cv.repaint();
        } else pha = null;
    }

    @Override
    protected void daKetNoi(KetNoiKhach k) {
        super.daKetNoi(k);
        batNut();
        if (!henHh.isRunning()) henHh.start();
    }

    @Override
    protected void matKetNoi(KetNoiKhach k, String lyDo) {
        super.matKetNoi(k, lyDo);
        if (kn == null) {
            trang = "san_sang";
            xeToi = null;
            chuyen = null;
            hoatHinh = null;
            batNut();
        }
    }

    @Override
    void dong() {
        henHh.stop();
        super.dong();
    }

    @Override
    protected String dongDinhKy() {
        Map<String, Object> x = xeToi != null ? xe(xeToi) : null;
        return tt.get("trong") + "/" + tt.get("so_xe") + " xe trống; trạng thái " + trang
                + (x != null ? ", xe " + x.get("ma") + " " + x.get("tai_xe") : "") + "; bạn đặt " + soDat + ", được " + soDuoc;
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43712;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new DatXeGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
