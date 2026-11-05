import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.GridLayout;
import java.awt.font.TextAttribute;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Path2D;
import java.awt.geom.RoundRectangle2D;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.ButtonGroup;
import javax.swing.DefaultListCellRenderer;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JRadioButton;
import javax.swing.JScrollPane;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.11 – Trang flash sale (ứng dụng người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ trang
 * "giờ vàng" như sàn thương mại điện tử: dải FLASH SALE với đồng hồ đếm ngược tới giờ mở bán (rồi "ĐANG DIỄN RA",
 * "CHÁY HÀNG"), thẻ sản phẩm (ảnh tai nghe vẽ bằng Graphics2D, nhãn giảm giá, giá sale, giá gốc gạch ngang), thanh tồn
 * kho "ĐÃ BÁN x% / SẮP CHÁY HÀNG", chọn số lượng, nút MUA NGAY (khóa tới giờ mở bán, "HẾT HÀNG" khi kho về 0), dòng
 * thông báo, đơn hàng của bạn và danh sách "vừa mua" trực tiếp (từ cap_nhat ≤ 10 lần/giây).
 *
 * Chạy:  java -cp out FlashSaleGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43752)
 */
public class FlashSaleGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color DO_SALE = new Color(0xe4002b), CAM_SALE = new Color(0xff6a00);

    /** Ảnh sản phẩm: hộp sạc và đôi tai nghe không dây, tâm (cx, cy), tỉ lệ k; nen: vẽ nền tròn đồng tâm. */
    static void veTaiNghe(Graphics2D g, double w, double h, double cx, double cy, double k, boolean nen) {
        if (nen) {
            g.setColor(new Color(0xfff1e6));
            g.fill(new java.awt.geom.Rectangle2D.Double(0, 0, w, h));
            int[] mau = {0xffe0cc, 0xffd1b3, 0xffc299};
            for (int i = 0; i < 3; i++) {
                double r = (95 - i * 22) * k;
                g.setColor(new Color(mau[i]));
                g.fill(new Ellipse2D.Double(cx - r, cy - r + 10 * k, 2 * r, 2 * r));
            }
        }
        double x0 = cx - 52 * k, x1 = cx + 52 * k, y0 = cy + 2 * k, y1 = cy + 62 * k, r = 22 * k;   // hộp sạc bo góc
        g.setColor(new Color(0xc9b8a8));                                                        // bóng
        g.fill(new RoundRectangle2D.Double(x0 + 3 * k, y0 + 3 * k, x1 - x0, y1 - y0, 2 * r, 2 * r));
        RoundRectangle2D hop = new RoundRectangle2D.Double(x0, y0, x1 - x0, y1 - y0, 2 * r, 2 * r);
        g.setColor(new Color(0xfafafa));
        g.fill(hop);
        g.setColor(new Color(0x9a9a9a));
        g.setStroke(new BasicStroke(1));
        g.draw(hop);
        g.setColor(new Color(0xb8b8b8));
        g.draw(new java.awt.geom.Line2D.Double(x0 + 4 * k, y0 + 22 * k, x1 - 4 * k, y0 + 22 * k));
        g.setColor(new Color(0x2ecc71));
        g.fill(new Ellipse2D.Double(cx - 3 * k, y0 + 34 * k, 6 * k, 6 * k));
        for (int d : new int[] {-1, 1}) {                                                        // hai tai nghe
            double hx = cx + d * 28 * k, hy = cy - 42 * k;
            Path2D than = new Path2D.Double();
            than.moveTo(hx - d * 4 * k, hy + 10 * k);
            than.lineTo(hx + d * 6 * k, hy + 10 * k);
            than.quadTo(hx + d * 10 * k, hy + 30 * k, hx + d * 12 * k, hy + 52 * k);
            than.quadTo(hx + d * 7 * k, hy + 56 * k, hx + d * 2 * k, hy + 54 * k);
            than.quadTo(hx - d * 2 * k, hy + 30 * k, hx - d * 4 * k, hy + 10 * k);
            g.setColor(new Color(0xf4f4f4));
            g.fill(than);
            g.setColor(new Color(0x9a9a9a));
            g.draw(than);
            Ellipse2D dau = new Ellipse2D.Double(hx - 17 * k, hy - 15 * k, 34 * k, 32 * k);
            g.setColor(Color.WHITE);
            g.fill(dau);
            g.setColor(new Color(0x9a9a9a));
            g.draw(dau);
            g.setColor(new Color(0x3a3a3a));
            g.fill(new Ellipse2D.Double(hx - 8 * k, hy - 6 * k, 16 * k, 14 * k));
            g.setColor(new Color(0x7a7a7a));
            g.fill(new Ellipse2D.Double(hx - 4 * k, hy - 3 * k, 5 * k, 4 * k));
        }
    }

    static String dongHo(double giay) {
        int s = Math.max(0, (int) (giay + 0.999));
        return Chung.f("%02d:%02d:%02d", s / 3600, s / 60 % 60, s % 60);
    }

    Double moLuc;                                                  // thời điểm mở bán theo đồng hồ máy này
    final List<Object[]> don = new ArrayList<>();                  // đơn của tôi: {mã, số lượng, thành tiền, giờ}
    boolean dangCho;
    JLabel nhanTrangThai, nhanDongHo, nhanTen, nhanGia, nhanGoc, nhanGioiHan, thongBao;
    GiaoDienChung.KhungVe cv, cvKho;
    JRadioButton[] chonSl = new JRadioButton[2];
    JButton nutMua;
    DefaultListModel<String> lsDon, lsGan;
    final List<Boolean> ganCuaToi = new ArrayList<>();

    FlashSaleGui(String ten, String host, int cong) {
        super("Flash sale", ten, host, cong);
        xay();
    }

    static JPanel nen(JPanel p, Color c) {
        p.setBackground(c);
        p.setAlignmentX(0);
        return p;
    }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new BoxLayout(khung, BoxLayout.Y_AXIS));
        JPanel dai = nen(new JPanel(new BorderLayout()), DO_SALE);
        dai.setBorder(BorderFactory.createEmptyBorder(4, 10, 4, 10));
        JLabel fs = GiaoDienChung.nhan("FLASH SALE  ", new Font(Font.DIALOG, Font.BOLD | Font.ITALIC, 26), Color.WHITE);
        JLabel gv = GiaoDienChung.nhan(GiaoDienChung.html("GIỜ VÀNG\nGIÁ SỐC"), GiaoDienChung.chu(12, true), new Color(0xffe066));
        dai.add(GiaoDienChung.hang(fs, gv), BorderLayout.WEST);
        ((JPanel) dai.getComponent(0)).setBackground(DO_SALE);
        nhanTrangThai = GiaoDienChung.nhan("Bắt đầu sau", GiaoDienChung.chu(13, true), Color.WHITE);
        nhanDongHo = GiaoDienChung.nhanNen("--:--:--", GiaoDienChung.chuDeu(20, true), new Color(0x1a1a1a), 0, 0);
        nhanDongHo.setForeground(Color.WHITE);
        nhanDongHo.setBorder(BorderFactory.createEmptyBorder(2, 6, 2, 6));
        JPanel ph = GiaoDienChung.hang(nhanTrangThai, nhanDongHo);
        ph.setBackground(DO_SALE);
        dai.add(ph, BorderLayout.EAST);
        dai.setMaximumSize(new Dimension(Integer.MAX_VALUE, 52));
        khung.add(Box.createVerticalStrut(4));
        khung.add(dai);

        JPanel the = nen(new JPanel(), Color.WHITE);
        the.setLayout(new BoxLayout(the, BoxLayout.X_AXIS));
        the.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLineBorder(new Color(0xd0d0d0)),
                BorderFactory.createEmptyBorder(6, 6, 6, 8)));
        cv = new GiaoDienChung.KhungVe(230, 210, this::veAnh);
        the.add(cv);
        the.add(Box.createHorizontalStrut(8));
        JPanel phai = nen(new JPanel(), Color.WHITE);
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        nhanTen = GiaoDienChung.nhan("–", GiaoDienChung.chu(16, true), null);
        phai.add(nhanTen);
        nhanGia = GiaoDienChung.nhan("–", GiaoDienChung.chu(28, true), DO_SALE);
        Font gach = GiaoDienChung.chu(14, false).deriveFont(Map.of(TextAttribute.STRIKETHROUGH, TextAttribute.STRIKETHROUGH_ON));
        nhanGoc = GiaoDienChung.nhan("", gach, new Color(0x909090));
        JPanel gia = GiaoDienChung.hang(nhanGia, Box.createHorizontalStrut(6), nhanGoc);
        gia.setBackground(Color.WHITE);
        phai.add(Box.createVerticalStrut(4));
        phai.add(gia);
        nhanGioiHan = GiaoDienChung.nhan("", GiaoDienChung.chu(12, false), new Color(0x606060));
        phai.add(nhanGioiHan);
        cvKho = new GiaoDienChung.KhungVe(300, 24, this::veKho);
        phai.add(Box.createVerticalStrut(6));
        phai.add(cvKho);
        ButtonGroup nhom = new ButtonGroup();
        JPanel sl = GiaoDienChung.hang(GiaoDienChung.nhan("Số lượng:"));
        sl.setBackground(Color.WHITE);
        for (int n = 1; n <= 2; n++) {
            JRadioButton r = new JRadioButton(String.valueOf(n), n == 1);
            r.setBackground(Color.WHITE);
            nhom.add(r);
            sl.add(r);
            chonSl[n - 1] = r;
        }
        phai.add(Box.createVerticalStrut(4));
        phai.add(sl);
        nutMua = GiaoDienChung.nut("CHỜ MỞ BÁN", CAM_SALE, 240);
        nutMua.setFont(GiaoDienChung.chu(18, true));
        nutMua.setForeground(Color.WHITE);
        nutMua.setPreferredSize(new Dimension(240, 42));
        nutMua.setMaximumSize(new Dimension(240, 42));
        nutMua.setAlignmentX(0);
        nutMua.addActionListener(e -> mua(null));
        phai.add(Box.createVerticalStrut(6));
        phai.add(nutMua);
        phai.setPreferredSize(new Dimension(320, 210));
        the.add(phai);
        khung.add(Box.createVerticalStrut(4));
        khung.add(the);
        thongBao = GiaoDienChung.nhanNen("Chưa kết nối", GiaoDienChung.chu(14, true), new Color(0xe0e0e0), 0, 0);
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        thongBao.setMaximumSize(new Dimension(Integer.MAX_VALUE, 30));
        thongBao.setPreferredSize(new Dimension(560, 30));
        khung.add(Box.createVerticalStrut(4));
        khung.add(thongBao);
        JPanel duoi = nen(new JPanel(new GridLayout(1, 2, 6, 0)), GiaoDienChung.NEN);
        lsDon = new DefaultListModel<>();
        lsGan = new DefaultListModel<>();
        duoi.add(nhomDs("Đơn hàng của bạn", lsDon, null));
        duoi.add(nhomDs("Vừa mua (trực tiếp)", lsGan, ganCuaToi));
        khung.add(Box.createVerticalStrut(4));
        khung.add(duoi);
        datNut();
    }

    static JPanel nhomDs(String tieuDe, DefaultListModel<String> m, List<Boolean> toi) {
        JPanel g = GiaoDienChung.khungNhom(tieuDe);
        g.setLayout(new BorderLayout());
        JList<String> l = new JList<>(m);
        l.setFont(GiaoDienChung.chuDeu(12, false));
        l.setVisibleRowCount(4);
        l.setFocusable(false);
        if (toi != null)
            l.setCellRenderer(new DefaultListCellRenderer() {
                @Override
                public Component getListCellRendererComponent(JList<?> ls, Object v, int i, boolean chon, boolean td) {
                    super.getListCellRendererComponent(ls, v, i, false, false);
                    setForeground(i < toi.size() && toi.get(i) ? new Color(0x0040c0) : Color.BLACK);
                    return this;
                }
            });
        JScrollPane c = new JScrollPane(l);
        c.setPreferredSize(new Dimension(270, 84));
        g.add(c);
        return g;
    }

    void veAnh(Graphics2D g) {
        veTaiNghe(g, 230, 210, 115, 92, 1.0, true);
        Map<String, Object> t = tt;
        if (t == null) return;
        Map<String, Object> sp = Chung.map(t.get("sp"));
        long giam = Math.round(100 * (1 - Chung.sod(sp.get("gia_sale")) / Chung.sod(sp.get("gia_goc"))));
        Ellipse2D nhan = new Ellipse2D.Double(168, 8, 54, 54);
        g.setColor(DO_SALE);
        g.fill(nhan);
        g.setColor(Color.WHITE);
        g.setStroke(new BasicStroke(2));
        g.draw(nhan);
        g.setFont(GiaoDienChung.chu(15, true));
        GiaoDienChung.chuGiua(g, "-" + giam + "%", 195, 35);
        if (Chung.so(t.get("ton")) <= 0) {
            g.setColor(new Color(0x404040));
            g.fillRect(0, 170, 230, 35);
            g.setColor(Color.WHITE);
            g.setFont(GiaoDienChung.chu(18, true));
            GiaoDienChung.chuGiua(g, "HẾT HÀNG", 115, 187);
        }
    }

    void veKho(Graphics2D g) {
        Map<String, Object> t = tt;
        g.setColor(Color.WHITE);
        g.fillRect(0, 0, 300, 24);
        if (t == null) return;
        long ton = Chung.so(t.get("ton")), dau = Chung.so(t.get("ton_dau"));
        double tiLe = Math.min(1.0, (double) (dau - ton) / dau);
        g.setColor(new Color(0xffd6c2));
        g.fillRect(0, 2, 300, 20);
        g.setColor(tiLe >= 0.8 ? DO_SALE : CAM_SALE);
        g.fillRect(0, 2, (int) Math.round(300 * tiLe), 20);
        String chu = ton <= 0 ? "HẾT HÀNG" : tiLe >= 0.8 ? "SẮP CHÁY HÀNG – còn " + ton : "ĐÃ BÁN " + Math.round(tiLe * 100) + "%";
        g.setColor(tiLe > 0.45 ? Color.WHITE : new Color(0xa03000));
        g.setFont(GiaoDienChung.chu(13, true));
        GiaoDienChung.chuGiua(g, chu, 150, 12);
    }

    // ------------------------------------------------------------ mua
    void mua(Integer sl) {
        if (kn == null || dangCho) return;
        if (sl != null) chonSl[sl - 1].setSelected(true);
        long n = chonSl[1].isSelected() ? 2 : 1;
        guiYc("mua", Json.obj("so_luong", n));                        // gửi trước, vẽ sau
        dangCho = true;
        nutMua.setText("ĐANG XỬ LÝ…");
        nutMua.setEnabled(false);
        nhatKy.ghi("MUA NGAY " + n + " sản phẩm");
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        dangCho = false;
        if (Chung.dung(tin.get("ok"))) {
            don.add(new Object[] {tin.get("ma_don"), tin.get("so_luong"), Chung.so(tin.get("thanh_tien")), gio()});
            bao("Đặt hàng thành công! Đơn " + tin.get("ma_don") + " – " + tin.get("so_luong") + " sp", new Color(0x70d070), Color.BLACK);
            nhatKy.ghi("  đơn " + tin.get("ma_don") + ", " + Chung.tien(Chung.so(tin.get("thanh_tien"))) + Chung.f(" (%.1f ms)", rtt));
            lsDon.clear();
            for (int i = don.size() - 1; i >= 0; i--) {
                Object[] d = don.get(i);
                lsDon.addElement(d[3] + " " + d[0] + " ×" + d[1] + " " + Chung.f("%11s", Chung.tien((long) d[2])));
            }
        } else {
            Object m = tin.get("ma_loi");
            String text = "chua_mo".equals(m) ? "Chưa tới giờ mở bán!"
                    : "het_hang".equals(m) ? "Rất tiếc, sản phẩm đã HẾT HÀNG"
                    : "khong_du".equals(m) ? "Chỉ còn " + tin.get("ton") + " sản phẩm – giảm số lượng"
                    : "xung_dot".equals(m) ? "Hệ thống đang bận, vui lòng thử lại"
                    : String.valueOf(tin.get("ly_do") == null ? "" : tin.get("ly_do"));
            boolean het = "het_hang".equals(m);
            bao(text, het ? new Color(0xff6060) : new Color(0xffb347), het ? Color.WHITE : Color.BLACK);
            nhatKy.ghi("  từ chối: " + tin.get("ly_do"));
        }
        datNut();
    }

    /** Giờ hiện tại HH:mm:ss (cùng định dạng với máy chủ). */
    static String gio() { return java.time.LocalTime.now().format(GiaoDienChung.GIO); }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() { return thongBao.getText(); }

    boolean daMo() { return moLuc != null && Chung.bayGio() >= moLuc; }

    void datNut() {
        if (dangCho) return;
        if (kn == null || tt == null) datNut("CHỜ MỞ BÁN", false, new Color(0xc8c8c8));
        else if (Chung.so(tt.get("ton")) <= 0) datNut("HẾT HÀNG", false, new Color(0x909090));
        else if (!daMo()) datNut("CHỜ MỞ BÁN", false, new Color(0xf5b48a));
        else datNut("MUA NGAY", true, CAM_SALE);
    }

    void datNut(String chu, boolean bat, Color nen) {
        if (!nutMua.getText().equals(chu)) nutMua.setText(chu);
        if (nutMua.isEnabled() != bat) nutMua.setEnabled(bat);
        nutMua.setBackground(nen);
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        Map<String, Object> sp = Chung.map(t.get("sp"));
        moLuc = Chung.bayGio() + Chung.sod(t.get("mo_con_ms")) / 1000;
        nhanTen.setText(String.valueOf(sp.get("ten")));
        nhanGia.setText(Chung.tien(Chung.so(sp.get("gia_sale"))));
        nhanGoc.setText(Chung.tien(Chung.so(sp.get("gia_goc"))));
        nhanGioiHan.setText("Mỗi đơn tối đa " + sp.get("toi_da") + " sản phẩm · Số lượng có hạn: " + t.get("ton_dau"));
        cv.repaint();
        cvKho.repaint();
        List<String> ls = new ArrayList<>();
        List<Boolean> toi = new ArrayList<>();
        for (Object o : Chung.list(t.get("don_gan"))) {
            Map<String, Object> x = Chung.map(o);
            ls.add(x.get("gio") + " " + Chung.f("%-6s", x.get("nguoi")) + " ×" + x.get("so_luong"));
            toi.add(ten.equals(x.get("nguoi")));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < lsGan.size(); i++) cu.add(lsGan.get(i));
        if (!cu.equals(ls)) {
            ganCuaToi.clear();
            ganCuaToi.addAll(toi);
            lsGan.clear();
            for (String x : ls) lsGan.addElement(x);
        }
        if (thongBaoText().equals("Chưa kết nối"))
            bao("Đón giờ vàng – nút MUA NGAY mở khi hết đếm ngược", new Color(0xe0e0e0), Color.BLACK);
        moiNhip();
    }

    @Override
    protected void suKien(String text) {
        super.suKien(text);
        if (text.startsWith("Đặt lại")) {                             // đợt sale mới
            don.clear();
            lsDon.clear();
            bao("Đợt sale mới – chờ giờ mở bán", new Color(0xe0e0e0), Color.BLACK);
        }
    }

    @Override
    protected void moiNhip() {
        if (tt == null || moLuc == null) return;
        double con = moLuc - Chung.bayGio();
        if (con > 0) {
            nhanTrangThai.setText("Bắt đầu sau");
            nhanDongHo.setText(dongHo(con));
            nhanDongHo.setBackground(new Color(0x1a1a1a));
        } else if (Chung.so(tt.get("ton")) <= 0) {
            nhanTrangThai.setText("Đã kết thúc");
            nhanDongHo.setText("CHÁY HÀNG");
            nhanDongHo.setBackground(new Color(0x404040));
        } else {
            nhanTrangThai.setText("");
            nhanDongHo.setText("ĐANG DIỄN RA");
            nhanDongHo.setBackground(GiaoDienChung.DO_DAM);
        }
        datNut();
    }

    @Override
    protected String dongDinhKy() {
        double con = Math.max(0.0, (moLuc == null ? 0 : moLuc) - Chung.bayGio());
        return "tồn " + tt.get("ton") + "/" + tt.get("ton_dau") + ", " + (con > 0 ? Chung.f("mở sau %.0f s", con) : "đang bán")
                + "; bạn có " + don.size() + " đơn";
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43752;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new FlashSaleGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
