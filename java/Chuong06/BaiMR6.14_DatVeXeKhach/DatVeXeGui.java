import java.awt.BasicStroke;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Line2D;
import java.awt.geom.Rectangle2D;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComboBox;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.14 – Ứng dụng đặt vé xe khách theo chặng (người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ ứng
 * dụng đặt vé: băng rôn nhà xe, chọn "Đi từ" – "Đến" (thanh tuyến Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang tô đoạn đã
 * chọn), sơ đồ xe (giường nằm 2 tầng hoặc ghế ngồi 50 chỗ, có vô lăng ở đầu xe). Mỗi chỗ tô màu theo ĐOẠN đang chọn:
 * trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn, và có dải nhỏ ở chân chỗ, mỗi chặng một vạch,
 * cho thấy chỗ đó bận ở chặng nào. Bấm chỗ để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm ngược (đỏ khi < 10 s),
 * "THANH TOÁN" và "Nhả chỗ". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây).
 *
 * Chạy:  java -cp out DatVeXeGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43782)
 */
public class DatVeXeGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color TRONG = new Color(0xffffff), CHON = new Color(0xb9f6ca), KHAC_GIU = new Color(0xffa726),
            DA_BAN = new Color(0x9e9e9e), TOI_GIU = new Color(0x2e7d32), VE_TOI = new Color(0x1565c0);
    static final Color MAU_XE = new Color(0x0b5d3b), NEN_XE = new Color(0xe9efeb), VIEN = new Color(0x607d74),
            CHU_TOI = new Color(0x263238);
    static final Object[][] CHU_GIAI = {{TRONG, "Trống"}, {CHON, "Đang chọn"}, {KHAC_GIU, "Người khác giữ"}, {DA_BAN, "Đã bán"},
            {TOI_GIU, "Bạn giữ"}, {VE_TOI, "Vé của bạn"}};
    static final Map<Character, Color> MAU_VACH = Map.of('.', new Color(0xdfe6e2), 'g', KHAC_GIU, 'b', DA_BAN, 'G', TOI_GIU,
            'V', VE_TOI);

    static String dongHo(double giay) {
        int s = Math.max(0, (int) (giay + 0.999));
        return Chung.f("%02d:%02d", s / 60, s % 60);
    }

    static String dongCua(Map<String, Object> tt, String g) {
        int i = Chung.list(tt.get("ghe")).indexOf(g);
        return i < 0 ? ".".repeat(MayChu.SO_CHANG) : String.valueOf(Chung.list(tt.get("so_do")).get(i));
    }

    /** Trạng thái chỗ g cho đoạn [tu, den): "trong" | "giu" | "ban" | "toi_giu" | "ve". */
    static String trangCho(Map<String, Object> tt, String g, String ten, int tu, int den) {
        String dong = dongCua(tt, g), kq = "trong";
        Map<String, Object> chu = Chung.map(tt.get("chu"));
        for (int k = tu; k < den; k++) {
            char x = dong.charAt(k);
            boolean toi = ten != null && ten.equals(chu.get(g + "#" + k));
            if (x == 'b' && !toi) return "ban";
            if (x == 'g' && !toi) kq = "giu";
            else if (x == 'g' && kq.equals("trong")) kq = "toi_giu";
            else if (x == 'b' && (kq.equals("trong") || kq.equals("toi_giu"))) kq = "ve";
        }
        return kq;
    }

    /** Một ký tự mỗi chặng: . g b (người khác) G V (của mình). */
    static String vach(Map<String, Object> tt, String g, String ten) {
        StringBuilder b = new StringBuilder(dongCua(tt, g));
        Map<String, Object> chu = Chung.map(tt.get("chu"));
        for (int k = 0; k < b.length(); k++)
            if (ten != null && ten.equals(chu.get(g + "#" + k)))
                b.setCharAt(k, b.charAt(k) == 'g' ? 'G' : b.charAt(k) == 'b' ? 'V' : b.charAt(k));
        return b.toString();
    }

    /** Vẽ sơ đồ xe; dùng ở ứng dụng đặt vé (to) và bảng điều khiển máy chủ (thu nhỏ, k < 1). */
    static final class SoDoXe {
        final String loaiXe;
        final double k;
        final boolean chu;
        final double rong, cao;
        final Map<String, double[]> viTri = new LinkedHashMap<>();
        final List<Object[]> tang = new ArrayList<>();         // {x0, x1, tên}

        SoDoXe(String loaiXe, double k, boolean chu) {
            this.loaiXe = loaiXe;
            this.k = k;
            this.chu = chu;
            double w, h, khe;
            if (loaiXe.equals("giuong")) {
                w = 46 * k;
                h = 30 * k;
                khe = 6 * k;
                double rongTang = 3 * w + 2 * khe;
                tang.add(new Object[]{12 * k, 12 * k + rongTang, "TẦNG DƯỚI"});
                tang.add(new Object[]{12 * k + rongTang + 26 * k, 12 * k + 2 * rongTang + 26 * k, "TẦNG TRÊN"});
                rong = 24 * k + 2 * rongTang + 26 * k;
                cao = 40 * k + 7 * (h + khe) + 4 * k;
            } else {
                w = 34 * k;
                h = 26 * k;
                khe = 5 * k;
                tang.add(new Object[]{12 * k, 12 * k + 6 * (w + khe) - khe, ""});
                rong = 24 * k + 6 * (w + khe) - khe;
                cao = 40 * k + 12 * (h + khe) + 4 * k;
            }
            for (MayChu.ChoXe x : MayChu.boTri(loaiXe)) {
                double px = (double) tang.get(loaiXe.equals("giuong") ? x.tang() : 0)[0] + x.cot() * (w + khe);
                double py = 40 * k + x.hang() * (h + khe);
                viTri.put(x.ma(), new double[]{px, py, px + w, py + h});
            }
        }

        void veNen(Graphics2D g) {
            g.setColor(NEN_XE);
            g.fill(new Rectangle2D.Double(0, 0, rong + 400, cao + 400));
            for (Object[] t : tang) {
                double x0 = (double) t[0], x1 = (double) t[1];
                Rectangle2D than = new Rectangle2D.Double(x0 - 6 * k, 8 * k, x1 - x0 + 12 * k, cao - 8 * k);
                g.setColor(new Color(0xf7faf8));
                g.fill(than);
                g.setColor(MAU_XE);
                g.setStroke(new BasicStroke((float) Math.max(1, 2 * k)));
                g.draw(than);
                if (chu && !((String) t[2]).isEmpty()) {
                    g.setFont(new Font(Font.DIALOG, Font.BOLD, 10));
                    GiaoDienChung.chuGiua(g, (String) t[2], (x0 + x1) / 2, 20 * k);
                }
            }
            if (chu || k >= 0.5) {                                   // vô lăng ở đầu xe (bên trái, tầng dưới)
                double r = 9 * k, cx = (double) tang.get(0)[0] + r + 2 * k, cy = 22 * k;
                g.setColor(CHU_TOI);
                g.draw(new Ellipse2D.Double(cx - r, cy - r, 2 * r, 2 * r));
                g.draw(new Line2D.Double(cx - r, cy, cx + r, cy));
            }
            g.setStroke(new BasicStroke(1));
        }

        /** Tô mọi chỗ theo đoạn [tu, den) và vẽ dải vạch theo chặng; vien: {chỗ: màu viền} (ghế nóng). */
        void ve(Graphics2D g, Map<String, Object> tt, String ten, int tu, int den, java.util.Set<String> chon, Map<String, Color> vien) {
            g.setFont(new Font(Font.DIALOG, Font.PLAIN, 9));
            List<Object> ds = tt == null ? List.of() : Chung.list(tt.get("ghe"));
            for (Map.Entry<String, double[]> e : viTri.entrySet()) {
                String ma = e.getKey();
                double[] r = e.getValue();
                String trang = "trong", vc = ".".repeat(MayChu.SO_CHANG);
                if (tt != null && ds.contains(ma)) {
                    trang = trangCho(tt, ma, ten, tu, den);
                    vc = vach(tt, ma, ten);
                }
                Color mau = switch (trang) {
                    case "giu" -> KHAC_GIU;
                    case "ban" -> DA_BAN;
                    case "toi_giu" -> TOI_GIU;
                    case "ve" -> VE_TOI;
                    default -> chon != null && chon.contains(ma) ? CHON : TRONG;
                };
                Rectangle2D o = new Rectangle2D.Double(r[0], r[1], r[2] - r[0], r[3] - r[1]);
                g.setColor(mau);
                g.fill(o);
                boolean nong = vien != null && vien.containsKey(ma);
                g.setColor(nong ? vien.get(ma) : VIEN);
                g.setStroke(new BasicStroke(nong ? 2 : 1));
                g.draw(o);
                g.setStroke(new BasicStroke(1));
                double caoVach = Math.max(2, 5 * k);
                for (int i = 0; i < vc.length(); i++) {               // dải vạch: mỗi chặng một ô ở chân chỗ
                    double a = r[0] + 2 + (r[2] - r[0] - 4) * i / vc.length(), b = r[0] + 2 + (r[2] - r[0] - 4) * (i + 1) / vc.length() - 1;
                    Rectangle2D v = new Rectangle2D.Double(a, r[3] - caoVach - 1, b - a, caoVach);
                    g.setColor(MAU_VACH.get(vc.charAt(i)));
                    g.fill(v);
                    if (tu <= i && i < den) {
                        g.setColor(CHU_TOI);
                        g.draw(v);
                    }
                }
                if (chu) {
                    boolean toi = mau.equals(TOI_GIU) || mau.equals(VE_TOI) || mau.equals(DA_BAN);
                    g.setColor(toi ? Color.WHITE : CHU_TOI);
                    GiaoDienChung.chuGiua(g, ma, (r[0] + r[2]) / 2, (r[1] + r[3]) / 2 - 3 * k);
                }
            }
        }

        String choTai(double x, double y) {
            for (Map.Entry<String, double[]> e : viTri.entrySet()) {
                double[] r = e.getValue();
                if (r[0] <= x && x <= r[2] && r[1] <= y && y <= r[3]) return e.getKey();
            }
            return null;
        }
    }

    record VeXe(String ghe, int tu, int den, String ma) {}

    // ------------------------------------------------------------ trạng thái cửa sổ
    TreeSet<String> chon, giu;         // chỗ đang chọn (chưa giữ); chỗ mình đang giữ (theo kq) cho đoạn doanGiu
    int[] doanGiu;
    List<VeXe> ve;                     // vé của mình
    long phienGiu, phienVe;            // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    Double han;                        // thời điểm (đồng hồ đơn điệu) hết hạn giữ chỗ
    int soGiu, soVe, tu, den;
    String bangRonCu;
    SoDoXe soDo;
    JComponent cv, cvXe, cvTuyen;
    JComboBox<String> chonTu, chonDen;
    JLabel nhanGhe, nhanTien, nhanDongHo, thongBao, nhanVe;
    JButton nutGiu, nutTt, nutNha;

    DatVeXeGui(String ten, String host, int cong) {
        super("Đặt vé xe khách", ten, host, cong);
        xay();
    }

    static JLabel dong(String chu, Font f, Color mau) {
        JLabel l = GiaoDienChung.nhan(chu, f, mau);
        l.setMaximumSize(new Dimension(Integer.MAX_VALUE, 200));
        return l;
    }

    static JButton nutLon(String chu, Color mau) {
        JButton b = GiaoDienChung.nut(chu, mau, 0);
        b.setForeground(Color.WHITE);
        b.setFont(GiaoDienChung.chu(14, true));
        b.setBorderPainted(false);
        b.setAlignmentX(0);
        b.setMaximumSize(new Dimension(Integer.MAX_VALUE, 32));
        return b;
    }

    @Override
    protected void ve(JPanel khung) {
        chon = new TreeSet<>();
        giu = new TreeSet<>();
        ve = new ArrayList<>();
        tu = 0;
        den = MayChu.SO_CHANG;
        khung.setLayout(new BoxLayout(khung, BoxLayout.X_AXIS));
        cv = new GiaoDienChung.KhungVe(376, 480, this::veCho);
        cv.addMouseListener(new MouseAdapter() {
            @Override public void mousePressed(MouseEvent e) {
                String g = soDo == null ? null : soDo.choTai(e.getX(), e.getY());
                if (g != null) bam(g);
            }
        });
        JPanel trai = GiaoDienChung.cot();
        trai.add(Box.createVerticalStrut(4));
        trai.add(cv);
        trai.add(Box.createVerticalGlue());
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(8));

        JPanel phai = new JPanel();
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        phai.setBackground(Color.WHITE);
        phai.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLineBorder(Color.BLACK),
                BorderFactory.createEmptyBorder(6, 6, 6, 6)));
        cvXe = new GiaoDienChung.KhungVe(256, 62, this::veBangRon);
        phai.add(cvXe);
        phai.add(Box.createVerticalStrut(4));
        chonTu = new JComboBox<>(MayChu.TRAM.subList(0, MayChu.SO_CHANG).toArray(new String[0]));
        chonDen = new JComboBox<>(MayChu.TRAM.subList(1, MayChu.TRAM.size()).toArray(new String[0]));
        chonDen.setSelectedIndex(MayChu.SO_CHANG - 1);
        chonTu.addActionListener(e -> doiDoan());
        chonDen.addActionListener(e -> doiDoan());
        for (Object[] p : new Object[][]{{"Đi từ", chonTu}, {"Đến", chonDen}}) {
            JComboBox<?> c = (JComboBox<?>) p[1];
            c.setMaximumSize(new Dimension(150, 26));
            c.setPreferredSize(new Dimension(150, 26));
            JLabel l = GiaoDienChung.nhan((String) p[0], GiaoDienChung.chu(12, false), null);
            l.setPreferredSize(new Dimension(44, 24));
            JPanel h = GiaoDienChung.hang(l, c);
            h.setBackground(Color.WHITE);
            phai.add(h);
        }
        cvTuyen = new GiaoDienChung.KhungVe(256, 40, this::veTuyen);
        phai.add(cvTuyen);
        nhanGhe = dong("Chỗ đã chọn: –", GiaoDienChung.chu(13, true), Color.BLACK);
        nhanTien = dong(" ", GiaoDienChung.chu(12, false), Color.BLACK);
        phai.add(nhanGhe);
        phai.add(nhanTien);
        phai.add(Box.createVerticalStrut(4));
        nutGiu = nutLon("GIỮ CHỖ", new Color(0xff8f00));
        nutGiu.addActionListener(e -> giuCho());
        phai.add(nutGiu);
        phai.add(Box.createVerticalStrut(2));
        nhanDongHo = GiaoDienChung.nhan("--:--", GiaoDienChung.chuDeu(24, true), new Color(0x909090));
        JPanel f = GiaoDienChung.hang(GiaoDienChung.nhan("Giữ chỗ còn", GiaoDienChung.chu(12, false), null), nhanDongHo);
        f.setBackground(Color.WHITE);
        phai.add(f);
        nutTt = nutLon("THANH TOÁN", MAU_XE);
        nutTt.addActionListener(e -> thanhToan());
        phai.add(nutTt);
        phai.add(Box.createVerticalStrut(2));
        nutNha = GiaoDienChung.nut("Nhả chỗ", new Color(0xf0f0f0), 0);
        nutNha.addActionListener(e -> nha());
        phai.add(nutNha);
        phai.add(Box.createVerticalStrut(2));
        thongBao = dong("Chưa kết nối", GiaoDienChung.chu(12, true), Color.BLACK);
        thongBao.setOpaque(true);
        thongBao.setBackground(new Color(0xe8e8e8));
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        thongBao.setBorder(BorderFactory.createEmptyBorder(4, 4, 4, 4));
        phai.add(thongBao);
        phai.add(Box.createVerticalStrut(2));
        nhanVe = dong("Vé của bạn: –", GiaoDienChung.chu(12, false), VE_TOI);
        phai.add(nhanVe);
        phai.add(Box.createVerticalGlue());
        JPanel boc = GiaoDienChung.cot();
        boc.add(Box.createVerticalStrut(4));
        boc.add(phai);
        khung.add(boc);
        bao("Chưa kết nối", new Color(0xe8e8e8), Color.BLACK);          // nạp sẵn bộ hiển thị HTML của Swing
        veLaiVe();
        capNhatGio();
    }

    // ------------------------------------------------------------ băng rôn, tuyến, sơ đồ
    void veBangRon(Graphics2D g) {
        g.setColor(MAU_XE);
        g.fillRect(0, 0, 256, 62);
        g.setColor(new Color(0x0f7a4f));
        g.fillRect(0, 44, 256, 18);
        if (tt == null) return;
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(14, true));
        g.drawString(String.valueOf(tt.get("nha_xe")).toUpperCase(), 10, 19);
        g.setColor(new Color(0xd7f0e2));
        g.setFont(GiaoDienChung.chu(11, false));
        g.drawString("Chuyến " + tt.get("chuyen") + " · " + tt.get("ten_xe"), 10, 37);
        g.setColor(new Color(0xffe082));
        g.setFont(GiaoDienChung.chu(11, true));
        g.drawString("Giữ chỗ " + Chung.g(Chung.sod(tt.get("ttl_s"))) + " s · giá theo chặng", 10, 57);
    }

    void veTuyen(Graphics2D g) {
        g.setColor(Color.WHITE);
        g.fillRect(0, 0, 256, 40);
        int n = MayChu.TRAM.size();
        double[] x = new double[n];
        for (int i = 0; i < n; i++) x[i] = 16 + i * (224.0 / (n - 1));
        g.setStroke(new BasicStroke(6));
        g.setColor(new Color(0xcfd8dc));
        g.draw(new Line2D.Double(x[0], 12, x[n - 1], 12));
        g.setColor(MAU_XE);
        g.draw(new Line2D.Double(x[tu], 12, x[den], 12));
        g.setStroke(new BasicStroke(2));
        List<Object> gia = tt == null ? List.of() : Chung.list(tt.get("gia_chang"));
        for (int i = 0; i < n; i++) {
            Ellipse2D c = new Ellipse2D.Double(x[i] - 6, 6, 12, 12);
            g.setColor(tu <= i && i <= den ? MAU_XE : Color.WHITE);
            g.fill(c);
            g.setColor(MAU_XE);
            g.draw(c);
            g.setColor(CHU_TOI);
            g.setFont(GiaoDienChung.chu(10, true));
            GiaoDienChung.chuGiua(g, MayChu.MA_TRAM.get(i), x[i], 30);
            if (i < gia.size() && i < n - 1) {
                g.setColor(VIEN);
                g.setFont(GiaoDienChung.chu(9, false));
                GiaoDienChung.chuGiua(g, Chung.so(gia.get(i)) / 1000 + "k", (x[i] + x[i + 1]) / 2, 30);
            }
        }
        g.setStroke(new BasicStroke(1));
    }

    void veCho(Graphics2D g) {
        g.setColor(NEN_XE);
        g.fillRect(0, 0, 376, 480);
        if (tt == null) return;
        String loai = String.valueOf(tt.get("loai_xe"));
        if (soDo == null || !soDo.loaiXe.equals(loai)) soDo = new SoDoXe(loai, 1, true);
        soDo.veNen(g);
        soDo.ve(g, tt, ten, tu, den, chon, null);
        double y = soDo.cao + 10, x = 8;
        g.setFont(GiaoDienChung.chu(11, false));
        for (int i = 0; i < CHU_GIAI.length; i++) {                // chú giải màu, 2 hàng
            if (i == 3) {
                x = 8;
                y += 16;
            }
            String chu = (String) CHU_GIAI[i][1];
            g.setColor((Color) CHU_GIAI[i][0]);
            g.fill(new Rectangle2D.Double(x, y, 14, 11));
            g.setColor(VIEN);
            g.draw(new Rectangle2D.Double(x, y, 14, 11));
            g.setColor(CHU_TOI);
            g.drawString(chu, (float) (x + 18), (float) (y + 10));
            x += 30 + g.getFontMetrics().stringWidth(chu);
        }
        List<String> chang = new ArrayList<>();
        for (int k = 0; k < MayChu.SO_CHANG; k++) chang.add(MayChu.MA_TRAM.get(k) + "-" + MayChu.MA_TRAM.get(k + 1));
        g.setColor(new Color(0x455a64));
        g.setFont(GiaoDienChung.chu(9, false));
        g.drawString("Vạch ở chân chỗ: từng chặng " + String.join(" | ", chang), 8, (float) (y + 28));
    }

    // ------------------------------------------------------------ chọn đoạn, chọn chỗ
    /** Chọn đoạn đường (người dùng hoặc kịch bản demo): chỉ số trạm. */
    void datDoan(int tu, int den) {
        chonTu.setSelectedIndex(tu);
        chonDen.setSelectedIndex(den - 1);
        doiDoan();
    }

    void doiDoan() {
        if (nutGiu == null) return;                               // đang dựng cửa sổ
        int t = chonTu.getSelectedIndex(), d = chonDen.getSelectedIndex() + 1;
        if (d <= t) {                                             // "Đến" phải sau "Đi từ"
            d = t + 1;
            chonDen.setSelectedIndex(d - 1);
        }
        tu = t;
        den = d;
        if (tt != null) {
            List<String> bo = new ArrayList<>();
            for (String g : chon) if (!trangCho(tt, g, ten, tu, den).equals("trong")) bo.add(g);
            chon.removeAll(bo);
            if (!bo.isEmpty())
                bao("Chỗ " + String.join(", ", bo) + " không trống cả đoạn " + MayChu.tenDoan(tu, den), new Color(0xffe0b2), Color.BLACK);
        }
        cvTuyen.repaint();
        capNhatGio();
    }

    /** Bấm một chỗ (người dùng hoặc kịch bản demo). */
    void bam(String g) {
        if (kn == null || tt == null) return;
        String trang = trangCho(tt, g, ten, tu, den);
        if (chon.contains(g)) chon.remove(g);
        else if (trang.equals("trong") && !giu.contains(g)) {
            if (chon.size() + giu.size() >= MayChu.TOI_DA_GHE) {
                bao("Tối đa " + MayChu.TOI_DA_GHE + " chỗ mỗi lần", new Color(0xffe0b2), Color.BLACK);
                return;
            }
            chon.add(g);
        } else if (trang.equals("giu")) bao("Chỗ " + g + " đang có người giữ trên đoạn này", new Color(0xffe0b2), Color.BLACK);
        else if (trang.equals("ban")) bao("Chỗ " + g + " đã bán trên đoạn này", new Color(0xe0e0e0), Color.BLACK);
        capNhatGio();
    }

    long gia(int tu, int den, int soCho) {
        if (tt == null) return 0;
        List<Object> g = Chung.list(tt.get("gia_chang"));
        long t = 0;
        for (int k = tu; k < den && k < g.size(); k++) t += Chung.so(g.get(k));
        return t * soCho;
    }

    void capNhatGio() {
        if (!giu.isEmpty()) {
            long g = gia(doanGiu[0], doanGiu[1], giu.size());
            nhanGhe.setText("Đang giữ: " + String.join(", ", giu));
            nhanTien.setText(MayChu.tenDoan(doanGiu[0], doanGiu[1]) + " · tổng " + Chung.tien(g));
            nutTt.setText("THANH TOÁN " + Chung.tien(g));
        } else {
            nhanGhe.setText("Chỗ đã chọn: " + (chon.isEmpty() ? "–" : String.join(", ", chon)));
            nhanTien.setText(MayChu.tenDoan(tu, den) + " · tạm tính " + Chung.tien(gia(tu, den, chon.size())));
            nutTt.setText("THANH TOÁN");
        }
        batNut();
        cv.repaint();
    }

    void batNut() {
        boolean co = kn != null, conHan = han != null && Chung.bayGio() < han;
        boolean g = co && !chon.isEmpty() && giu.isEmpty(), t = co && !giu.isEmpty() && conHan;
        nutGiu.setEnabled(g);
        nutGiu.setBackground(g ? new Color(0xff8f00) : new Color(0xffcc80));
        nutTt.setEnabled(t);
        nutTt.setBackground(t ? MAU_XE : new Color(0x8fb8a3));
        nutNha.setEnabled(co && !giu.isEmpty());
        chonTu.setEnabled(giu.isEmpty());                         // đang giữ chỗ: không đổi đoạn
        chonDen.setEnabled(giu.isEmpty());
    }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(GiaoDienChung.html(text).replace("<html>", "<html><body style='width:220px;text-align:center'>")
                .replace("</html>", "</body></html>"));
        thongBao.putClientProperty("chu", text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() {
        Object c = thongBao.getClientProperty("chu");
        return c == null ? thongBao.getText() : c.toString();
    }

    // ------------------------------------------------------------ giữ / thanh toán / nhả
    static Map<String, Object> truong(java.util.Collection<String> ghe, int tu, int den) {
        return Json.obj("ghe", new ArrayList<Object>(ghe), "tu", (long) tu, "den", (long) den);
    }

    void giuCho() {
        if (chon.isEmpty() || !giu.isEmpty()) return;
        guiYc("giu_cho", truong(chon, tu, den));                   // gửi trước, vẽ sau
        nhatKy.ghi("Giữ chỗ " + String.join(", ", chon) + " " + MayChu.tenDoan(tu, den) + "…");
        bao("Đang giữ chỗ…", new Color(0xfff3c4), Color.BLACK);
        nutGiu.setEnabled(false);
    }

    void thanhToan() {
        if (!giu.isEmpty()) {
            guiYc("thanh_toan", truong(giu, doanGiu[0], doanGiu[1]));
            nhatKy.ghi("Thanh toán " + String.join(", ", giu) + "…");
        }
    }

    void nha() {
        if (!giu.isEmpty()) guiYc("nha", truong(giu, doanGiu[0], doanGiu[1]));
    }

    static List<String> chuoi(Object ds) {
        List<String> p = new ArrayList<>();
        if (ds instanceof List<?> l) for (Object o : l) p.add(String.valueOf(o));
        return p;
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        List<String> ghe = chuoi(truong.get("ghe"));
        int t = (int) Chung.so(truong.get("tu")), d = (int) Chung.so(truong.get("den"));
        String noi = String.join(", ", ghe);
        if (loai.equals("giu_cho")) {
            if (Chung.dung(tin.get("ok"))) {
                soGiu++;
                giu = new TreeSet<>(chuoi(tin.get("ghe")));
                doanGiu = new int[]{t, d};
                phienGiu = tin.containsKey("phien") ? Chung.so(tin.get("phien")) : 0;
                chon.removeAll(giu);
                han = Chung.bayGio() + Chung.sod(tin.get("het_han_ms")) / 1000;
                bao("Đã giữ " + noi + Chung.f(" – thanh toán trong %.0f s", Chung.sod(tin.get("het_han_ms")) / 1000),
                        new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                nhatKy.ghi("  ĐÃ GIỮ " + noi + Chung.f(" (%.1f ms)", rtt));
                kiemCho(tt);
            } else {
                chon.removeAll(tin.containsKey("ghe_ban") ? chuoi(tin.get("ghe_ban")) : ghe);
                bao("Không giữ được: " + tin.get("ly_do"), new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                nhatKy.ghi("  không giữ được: " + tin.get("ly_do"));
            }
        } else if (loai.equals("thanh_toan")) {
            if (Chung.dung(tin.get("ok"))) {
                soVe += ghe.size();
                List<String> ma = chuoi(tin.get("ma_ve"));
                for (int i = 0; i < ghe.size(); i++) ve.add(new VeXe(ghe.get(i), t, d, i < ma.size() ? ma.get(i) : ""));
                phienVe = Chung.so(tin.get("phien"));
                giu.clear();
                han = null;
                bao("Đặt vé thành công! " + Chung.tien(Chung.so(tin.get("tien"))), new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                nhatKy.ghi("  ĐẶT VÉ THÀNH CÔNG: " + String.join(", ", ma) + Chung.f(" (%.1f ms)", rtt));
            } else {
                bao("Thanh toán bị từ chối: " + tin.get("ly_do"), new Color(0xff6060), Color.WHITE);
                nhatKy.ghi("  thanh toán bị từ chối: " + tin.get("ly_do"));
            }
        } else {
            if (Chung.dung(tin.get("ok"))) {
                List<String> da = chuoi(tin.get("ghe"));
                giu.removeAll(da);
                if (giu.isEmpty()) han = null;
                bao("Đã nhả " + String.join(", ", da), new Color(0xe8e8e8), Color.BLACK);
                nhatKy.ghi("  đã nhả " + String.join(", ", da));
            } else nhatKy.ghi("  nhả bị từ chối: " + tin.get("ly_do"));
        }
        veLaiVe();
        capNhatGio();
    }

    void veLaiVe() {
        List<String> ds = new ArrayList<>();
        for (VeXe v : ve) ds.add(v.ghe() + " " + MayChu.MA_TRAM.get(v.tu()) + "→" + MayChu.MA_TRAM.get(v.den()));
        nhanVe.setText(GiaoDienChung.html("Vé của bạn: " + (ds.isEmpty() ? "–" : String.join(", ", ds))
                + "\nBạn: giữ được " + soGiu + " lần, mua " + soVe + " vé"));
    }

    /** Chỗ mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; chỗ đang chọn bị người khác giữ. Chỉ tin cap_nhat
     *  chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán). */
    void kiemCho(Map<String, Object> t) {
        if (t == null) return;
        Map<String, Object> chu = Chung.map(t.get("chu"));
        long ph = Chung.so(t.get("phien"));
        if (ph >= phienGiu && !giu.isEmpty()) {
            for (String g : new ArrayList<>(giu)) {
                boolean tatCa = true;
                Object ai = null;
                for (int k = doanGiu[0]; k < doanGiu[1]; k++) {
                    Object a = chu.get(g + "#" + k);
                    if (!ten.equals(a)) {
                        tatCa = false;
                        if (a != null && ai == null) ai = a;
                    }
                }
                if (tatCa) continue;
                giu.remove(g);
                if (ai != null) {
                    bao("Chỗ " + g + " đã bị " + ai + " lấy mất!", new Color(0xff6060), Color.WHITE);
                    nhatKy.ghi("Chỗ " + g + " đã bị " + ai + " lấy mất!");
                } else {
                    bao("Hết thời gian giữ chỗ – chỗ đã được nhả", new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                    nhatKy.ghi("Hết thời gian giữ chỗ: " + g);
                }
            }
            if (giu.isEmpty()) han = null;
        }
        if (ph >= phienVe) {
            List<VeXe> con = new ArrayList<>();
            for (VeXe v : ve) {
                Object ai = ten;
                for (int k = v.tu(); k < v.den(); k++)
                    if (!ten.equals(chu.get(v.ghe() + "#" + k))) {
                        ai = chu.get(v.ghe() + "#" + k);
                        break;
                    }
                if (ten.equals(ai)) {
                    con.add(v);
                    continue;
                }
                String ai2 = ai == null ? "không ai" : String.valueOf(ai);
                bao("Vé " + v.ghe() + " bị ghi cho " + ai2 + "!", new Color(0xff6060), Color.WHITE);
                nhatKy.ghi("Vé " + v.ghe() + " bị ghi cho " + ai2 + "!");
            }
            ve = con;
        }
        List<String> biLay = new ArrayList<>();
        for (String g : chon) if (!trangCho(t, g, ten, tu, den).equals("trong")) biLay.add(g);
        chon.removeAll(biLay);
        if (!biLay.isEmpty()) {
            bao("Chỗ " + String.join(", ", biLay) + " vừa có người giữ", new Color(0xffe0b2), Color.BLACK);
            nhatKy.ghi("Chỗ " + String.join(", ", biLay) + " vừa có người giữ");
        }
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        String br = t.get("nha_xe") + "|" + t.get("chuyen") + "|" + t.get("ten_xe") + "|" + t.get("ttl_s");
        if (!br.equals(bangRonCu)) {
            bangRonCu = br;
            cvXe.repaint();
            cvTuyen.repaint();
        }
        if (thongBaoText().equals("Chưa kết nối")) bao("Chọn đoạn đường, chọn chỗ rồi bấm GIỮ CHỖ", new Color(0xe8e8e8), Color.BLACK);
        Map<String, Object> chu = Chung.map(t.get("chu")), dem = Chung.map(t.get("dem"));
        if (chu.isEmpty() && (!ve.isEmpty() || !giu.isEmpty()) && Chung.so(dem.get("ban")) == 0 && Chung.so(dem.get("giu")) == 0)
            ve.clear();                                              // máy chủ đặt lại: xe trống
        List<Object> ghe = Chung.list(t.get("ghe"));
        chon.removeIf(g -> !ghe.contains(g));                        // đổi loại xe: bỏ chỗ không còn
        if (!giu.isEmpty()) {
            Map<String, Object> conLai = Chung.map(t.get("con_lai_ms"));
            Double min = null;
            for (String g : giu)
                for (int k = doanGiu[0]; k < doanGiu[1]; k++) {
                    String o = g + "#" + k;
                    if (ten.equals(chu.get(o)) && conLai.containsKey(o)) {
                        double x = Chung.sod(conLai.get(o));
                        min = min == null ? x : Math.min(min, x);
                    }
                }
            if (min != null) han = Chung.bayGio() + min / 1000;
        }
        kiemCho(t);
        veLaiVe();
        capNhatGio();
    }

    @Override
    protected void moiNhip() {
        if (han == null) {
            nhanDongHo.setText("--:--");
            nhanDongHo.setForeground(new Color(0x909090));
            return;
        }
        double con = han - Chung.bayGio();
        nhanDongHo.setText(dongHo(con));
        nhanDongHo.setForeground(con < 10 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        if (con <= 0 && nutTt.isEnabled()) {
            batNut();
            bao("Hết thời gian giữ chỗ", new Color(0xffd0d0), GiaoDienChung.DO_DAM);
        }
    }

    @Override
    protected String dongDinhKy() {
        Map<String, Object> d = Chung.map(tt.get("dem"));
        String con = han == null ? "–" : Chung.f("%.0f s", Math.max(0, han - Chung.bayGio()));
        return "chỗ-chặng trống " + d.get("trong") + ", giữ " + d.get("giu") + ", bán " + d.get("ban") + "; bạn giữ "
                + (giu.isEmpty() ? "–" : String.join(", ", giu)) + " (còn " + con + "), vé " + ve.size();
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43782;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new DatVeXeGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
