import java.awt.BasicStroke;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Graphics2D;
import java.awt.geom.Arc2D;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Path2D;
import java.awt.geom.Rectangle2D;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.DefaultListCellRenderer;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.6 – Phòng đấu giá (ứng dụng người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ
 * phòng đấu giá: ảnh món hàng (vẽ bằng Graphics2D), giá hiện tại cỡ lớn, người dẫn đầu, đồng hồ đếm ngược (đỏ khi còn
 * < 10 s, báo GIA HẠN), các nút "+1/+2/+5 bước", ô nhập giá, lịch sử trả giá, thông báo "Bạn bị trả giá cao hơn!" /
 * "BẠN THẮNG!". Mọi số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây); đồng hồ đếm ngược chạy cục bộ từ
 * con_lai_ms của lần cap_nhat mới nhất.
 *
 * Chạy:  java -cp out DauGiaGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43702)
 */
public class DauGiaGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color LAM = new Color(0x1f4fa0);                 // màu men lam (gốm hoa lam)
    /** Nửa bề rộng của bình theo chiều cao (đơn vị trước khi nhân tỉ lệ): (y, nửa rộng). */
    static final double[][] DANG_BINH = {{0, 24}, {6, 22}, {14, 15}, {38, 16}, {52, 34}, {70, 50}, {92, 55}, {118, 47},
            {142, 32}, {156, 22}, {162, 25}, {170, 25}};

    static double nuaRong(double y) {
        for (int i = 0; i + 1 < DANG_BINH.length; i++) {
            double y0 = DANG_BINH[i][0], r0 = DANG_BINH[i][1], y1 = DANG_BINH[i + 1][0], r1 = DANG_BINH[i + 1][1];
            if (y0 <= y && y <= y1) return r0 + (r1 - r0) * (y - y0) / (y1 - y0);
        }
        return 0;
    }

    /** Đa giác "smooth" như Tk: B-spline bậc hai qua trung điểm các cạnh (kín). */
    static Path2D tron(double[][] p, boolean kin) {
        Path2D.Double s = new Path2D.Double();
        int n = p.length;
        if (kin) {
            s.moveTo((p[n - 1][0] + p[0][0]) / 2, (p[n - 1][1] + p[0][1]) / 2);
            for (int i = 0; i < n; i++) {
                double[] a = p[i], b = p[(i + 1) % n];
                s.quadTo(a[0], a[1], (a[0] + b[0]) / 2, (a[1] + b[1]) / 2);
            }
            s.closePath();
        } else {
            s.moveTo(p[0][0], p[0][1]);
            for (int i = 1; i < n - 1; i++) {
                double[] a = p[i], b = p[i + 1];
                if (i == n - 2) s.quadTo(a[0], a[1], b[0], b[1]);
                else s.quadTo(a[0], a[1], (a[0] + b[0]) / 2, (a[1] + b[1]) / 2);
            }
        }
        return s;
    }

    static void oval(Graphics2D g, double x0, double y0, double x1, double y1, Color fill, Color vien) {
        Ellipse2D e = new Ellipse2D.Double(x0, y0, x1 - x0, y1 - y0);
        if (fill != null) { g.setColor(fill); g.fill(e); }
        if (vien != null) { g.setColor(vien); g.setStroke(new BasicStroke(1)); g.draw(e); }
    }

    /** Vẽ "Bình gốm hoa lam": tâm ngang cx, đỉnh miệng ở y = tren, tỉ lệ k, trên ô rộng w, cao h. */
    static void veBinhGom(Graphics2D g, int w, int h, double cx, double tren, double k, boolean nen) {
        if (nen) {                                                  // nền tối + quầng đèn chiếu
            g.setColor(new Color(0x23232f));
            g.fillRect(0, 0, w, h);
            int[] mau = {0x2c2c3c, 0x363648, 0x404056, 0x4a4a62};
            for (int i = 0; i < mau.length; i++) {
                double r = (1.0 - i * 0.2) * w * 0.5;
                oval(g, cx - r, tren + 80 * k - r * 0.9, cx + r, tren + 80 * k + r * 0.9, new Color(mau[i]), null);
            }
            double day = tren + 170 * k;
            Path2D be = new Path2D.Double();
            be.moveTo(cx - 70 * k, day);
            be.lineTo(cx + 70 * k, day);
            be.lineTo(cx + 82 * k, day + 22 * k);
            be.lineTo(cx - 82 * k, day + 22 * k);
            be.closePath();
            g.setColor(new Color(0x6b4226));
            g.fill(be);
            g.setColor(new Color(0x3e2412));
            g.draw(be);
            g.setColor(new Color(0x4e2f1a));
            g.fill(new Rectangle2D.Double(cx - 82 * k, day + 22 * k, 164 * k, h - day - 22 * k));
            oval(g, cx - 48 * k, day - 5 * k, cx + 48 * k, day + 6 * k, new Color(0x1a1a1a), null);
        }
        int n = DANG_BINH.length;
        double[][] p = new double[2 * n][];
        for (int i = 0; i < n; i++) p[i] = new double[] {cx - DANG_BINH[i][1] * k, tren + DANG_BINH[i][0] * k};
        for (int i = 0; i < n; i++) p[n + i] = new double[] {cx + DANG_BINH[n - 1 - i][1] * k, tren + DANG_BINH[n - 1 - i][0] * k};
        Path2D than = tron(p, true);
        g.setColor(new Color(0xf5f2e8));
        g.fill(than);
        g.setColor(new Color(0x8c96a8));
        g.setStroke(new BasicStroke(1));
        g.draw(than);
        oval(g, cx - 24 * k, tren - 4 * k, cx + 24 * k, tren + 5 * k, new Color(0xdcd8cc), new Color(0x8c96a8));
        for (int y : new int[] {10, 36, 54, 60, 138, 150}) {     // đường viền men lam quanh thân
            double r = nuaRong(y) * k - 1;
            g.setColor(LAM);
            g.setStroke(new BasicStroke((float) Math.max(1, (int) ((y == 54 || y == 138 ? 2 : 1) * k))));
            g.draw(new java.awt.geom.Line2D.Double(cx - r, tren + y * k, cx + r, tren + y * k));
        }
        g.setStroke(new BasicStroke(1));
        for (int i = -3; i <= 3; i++) {                             // cánh sen quanh cổ
            double x = cx + i * 9 * k;
            g.setColor(LAM);
            g.draw(new Arc2D.Double(x - 5 * k, tren + 18 * k, 10 * k, 20 * k, 0, 180, Arc2D.OPEN));
        }
        double tamY = tren + 96 * k;                                // bông hoa giữa thân
        for (int i = 0; i < 8; i++) {
            double a = i * Math.PI / 4, x = cx + 15 * k * Math.cos(a), y = tamY + 15 * k * Math.sin(a);
            oval(g, x - 7 * k, y - 7 * k, x + 7 * k, y + 7 * k, new Color(0x5b82c4), LAM);
        }
        oval(g, cx - 8 * k, tamY - 8 * k, cx + 8 * k, tamY + 8 * k, LAM, LAM);
        for (int d : new int[] {-1, 1}) {                           // dây lá cuốn hai bên
            double[][] q = {{cx + d * 24 * k, tamY}, {cx + d * 34 * k, tamY - 16 * k}, {cx + d * 44 * k, tamY - 4 * k},
                    {cx + d * 40 * k, tamY + 14 * k}, {cx + d * 30 * k, tamY + 22 * k}};
            g.setColor(LAM);
            g.setStroke(new BasicStroke(2));
            g.draw(tron(q, false));
            g.setStroke(new BasicStroke(1));
            oval(g, cx + d * 40 * k - 4 * k, tamY - 12 * k, cx + d * 40 * k + 4 * k, tamY - 4 * k, LAM, LAM);
        }
        for (int i = -4; i <= 4; i++) {                             // cánh sen chân bình
            double x = cx + i * 8 * k;
            Path2D l = new Path2D.Double();
            l.moveTo(x - 4 * k, tren + 150 * k);
            l.lineTo(x, tren + 140 * k);
            l.lineTo(x + 4 * k, tren + 150 * k);
            g.setColor(LAM);
            g.draw(l);
        }
    }

    /** Đếm ngược dạng mm:ss (dưới 10 s thêm phần mười). */
    static String dongHo(double giay) {
        if (giay < 10) return Chung.f("00:%04.1f", giay);
        int s = (int) giay;
        return Chung.f("%02d:%02d", s / 60, s % 60);
    }

    record DongLs(String chu, boolean cuaToi) {}

    static JList<DongLs> danhSach(int cao, int rong, int co) {
        JList<DongLs> l = new JList<>(new DefaultListModel<>());
        l.setFont(GiaoDienChung.chuDeu(co, false));
        l.setVisibleRowCount(cao);
        l.setFixedCellWidth(rong);
        l.setFocusable(false);
        l.setCellRenderer(new DefaultListCellRenderer() {
            @Override
            public Component getListCellRendererComponent(JList<?> list, Object v, int i, boolean chon, boolean td) {
                super.getListCellRendererComponent(list, ((DongLs) v).chu(), i, false, false);
                setForeground(((DongLs) v).cuaToi() ? new Color(0x0040c0) : Color.BLACK);
                return this;
            }
        });
        return l;
    }

    // ------------------------------------------------------------ trạng thái của cửa sổ
    Double hetHan;                                   // thời điểm (đồng hồ đơn điệu) hết phiên theo cap_nhat
    long soLanCu;
    boolean danDau;
    int soTra, soNhan;
    double hanBaoGiaHan;
    JLabel nhanTenMon, nhanGiaKd, nhanCuaToi, nhanGia, nhanDanDau, nhanDongHo, nhanGiaHan, thongBao;
    final List<Object[]> nutBuoc = new ArrayList<>();   // {n, JButton}
    JTextField oGia;
    JButton nutTra;
    JList<DongLs> ls;

    DauGiaGui(String ten, String host, int cong) {
        super("Phòng đấu giá", ten, host, cong);
        xay();
    }

    JButton nutBuoc(int i) { return (JButton) nutBuoc.get(i)[1]; }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new javax.swing.BoxLayout(khung, javax.swing.BoxLayout.X_AXIS));
        JPanel trai = GiaoDienChung.cot();
        trai.add(new GiaoDienChung.KhungVe(230, 240, g -> veBinhGom(g, 230, 240, 115, 26, 1.0, true)));
        nhanTenMon = GiaoDienChung.nhan("–", GiaoDienChung.chu(14, true), null);
        trai.add(Box.createVerticalStrut(4));
        trai.add(nhanTenMon);
        nhanGiaKd = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, false), null);
        trai.add(nhanGiaKd);
        nhanCuaToi = GiaoDienChung.nhan("Bạn: chưa trả giá", GiaoDienChung.chu(12, false), null);
        trai.add(nhanCuaToi);
        trai.add(Box.createVerticalGlue());
        trai.setBorder(BorderFactory.createEmptyBorder(4, 0, 4, 0));
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(8));

        JPanel phai = GiaoDienChung.cot();
        phai.setBorder(BorderFactory.createEmptyBorder(4, 0, 4, 0));
        phai.add(GiaoDienChung.nhan("GIÁ HIỆN TẠI", GiaoDienChung.chu(12, true), new Color(0x404040)));
        nhanGia = GiaoDienChung.nhan("–", GiaoDienChung.chu(32, true), new Color(0x8b0000));
        phai.add(nhanGia);
        nhanDanDau = GiaoDienChung.nhan("Dẫn đầu: –", GiaoDienChung.chu(13, false), null);
        phai.add(nhanDanDau);
        nhanDongHo = GiaoDienChung.nhan("–", GiaoDienChung.chuDeu(26, true), null);
        nhanGiaHan = GiaoDienChung.nhan("", GiaoDienChung.chu(13, true), GiaoDienChung.DO_DAM);
        phai.add(GiaoDienChung.hang(GiaoDienChung.nhan("Còn lại ", GiaoDienChung.chu(13, false), null), nhanDongHo,
                Box.createHorizontalStrut(4), nhanGiaHan));
        thongBao = GiaoDienChung.nhanNen("Chưa kết nối", GiaoDienChung.chu(14, true), new Color(0xe0e0e0), 372, 30);
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        phai.add(Box.createVerticalStrut(4));
        phai.add(thongBao);
        phai.add(Box.createVerticalStrut(4));
        JPanel f = GiaoDienChung.hang();
        for (int n : new int[] {1, 2, 5}) {
            JButton b = GiaoDienChung.nut("<html><center>+" + n + " bước</center></html>",
                    n == 1 ? GiaoDienChung.CAM : GiaoDienChung.XANH_DUONG, 118);
            b.setPreferredSize(new Dimension(118, 42));
            b.addActionListener(e -> tang(n));
            f.add(b);
            nutBuoc.add(new Object[] {n, b});
        }
        phai.add(f);
        oGia = GiaoDienChung.oNhap("", 11);
        nutTra = GiaoDienChung.nut("Trả giá", new Color(0xff8c00), 90);
        nutTra.addActionListener(e -> traONhap());
        phai.add(GiaoDienChung.hang(GiaoDienChung.nhan("Giá của bạn:"), oGia, nutTra));
        JPanel g = GiaoDienChung.khungNhom("Lịch sử trả giá (mới nhất trên cùng)");
        g.setLayout(new java.awt.BorderLayout());
        ls = danhSach(8, 340, 12);
        g.add(new JScrollPane(ls));
        g.setAlignmentX(0);
        phai.add(g);
        khung.add(phai);
        batNut(false);
    }

    // ------------------------------------------------------------ trả giá
    void batNut(boolean bat) {
        for (Object[] x : nutBuoc) ((JButton) x[1]).setEnabled(bat);
        nutTra.setEnabled(bat);
        veNutBuoc();
    }

    /** Chữ hai dòng của nút "+n bước" (HTML); nút tắt thì chữ xám (Swing không tự làm mờ chữ HTML). */
    void veNutBuoc() {
        for (Object[] x : nutBuoc) {
            JButton b = (JButton) x[1];
            String gia = tt == null ? "" : "<br>" + Chung.tien(Chung.so(tt.get("gia"))
                    + (int) x[0] * Chung.so(Chung.map(tt.get("mon")).get("buoc_gia")));
            b.setText("<html><center><font color='" + (b.isEnabled() ? "#000000" : "#7a7a7a") + "'>+" + x[0] + " bước"
                    + gia + "</font></center></html>");
        }
    }

    void tang(int n) {
        if (tt != null) tra(Chung.so(tt.get("gia")) + n * Chung.so(Chung.map(tt.get("mon")).get("buoc_gia")));
    }

    void traONhap() {
        long gia;
        try {
            gia = Long.parseLong(oGia.getText().replace(".", "").replace(" ", "").replace("đ", ""));
        } catch (NumberFormatException e) {
            nhatKy.ghi("Giá phải là số nguyên (đồng)");
            return;
        }
        tra(gia);
    }

    void tra(long gia) {
        soTra++;
        nhatKy.ghi("Trả " + Chung.tien(gia) + "…");
        guiYc("tra_gia", Json.obj("gia", gia));
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        long gia = Chung.so(truong.get("gia"));
        if (Chung.dung(tin.get("ok"))) {
            soNhan++;
            nhatKy.ghi("  chấp nhận " + Chung.tien(gia) + Chung.f(" (%.1f ms)", rtt)
                    + (Chung.dung(tin.get("gia_han")) ? " – phiên được GIA HẠN" : ""));
            bao("Bạn đang dẫn đầu", new Color(0x70d070), Color.BLACK);
            danDau = true;                                        // cap_nhat sau đó báo nếu bị vượt (hoặc bị ghi đè)
        } else {
            nhatKy.ghi("  từ chối " + Chung.tien(gia) + ": " + tin.get("ly_do"));
            bao("Bị từ chối: " + ("gia_thap".equals(tin.get("ma_loi")) ? "giá đã thay đổi" : String.valueOf(tin.get("ly_do"))),
                    new Color(0xffb347), Color.BLACK);
        }
        nhanCuaToi.setText("Bạn: trả " + soTra + " lần, được nhận " + soNhan);
    }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() { return thongBao.getText(); }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        Map<String, Object> mon = Chung.map(t.get("mon"));
        long gia = Chung.so(t.get("gia")), buoc = Chung.so(mon.get("buoc_gia")), soLan = Chung.so(t.get("so_lan"));
        nhanTenMon.setText(String.valueOf(mon.get("ten")));
        nhanGiaKd.setText("Khởi điểm " + Chung.tien(Chung.so(mon.get("gia_khoi_diem"))) + " · bước " + Chung.tien(buoc));
        nhanGia.setText(Chung.tien(gia));
        Object ai = t.get("nguoi_dan_dau");
        boolean ketThuc = Chung.dung(t.get("ket_thuc"));
        nhanDanDau.setText("Dẫn đầu: " + (ai == null ? "–" : ai) + "  (" + soLan + " lượt trả)");
        Object cl = t.get("con_lai_ms");
        Double moi = cl == null ? null : Chung.bayGio() + Chung.sod(cl) / 1000;
        if (moi != null && hetHan != null && moi > hetHan + 0.5 && !ketThuc && soLan > soLanCu) {   // hạn lùi vì có giá mới
            hanBaoGiaHan = Chung.bayGio() + 3.0;
            nhatKy.ghi(Chung.f("GIA HẠN: còn %.1f s", Chung.sod(cl) / 1000));
        }
        hetHan = moi;
        soLanCu = soLan;
        DefaultListModel<DongLs> m = (DefaultListModel<DongLs>) ls.getModel();
        m.clear();
        for (Object o : Chung.list(t.get("lich_su"))) {
            Map<String, Object> x = Chung.map(o);
            m.addElement(new DongLs(Chung.f("%s %-6s %14s", x.get("gio"), x.get("nguoi"), Chung.tien(Chung.so(x.get("gia"))))
                    + (Chung.dung(x.get("gia_han")) ? " gia hạn" : ""), ten.equals(x.get("nguoi"))));
        }
        boolean cuaToi = ten.equals(ai);
        if (ketThuc) {
            batNut(false);
            if (cuaToi) bao("BẠN THẮNG! " + Chung.tien(gia), new Color(0xffd700), Color.BLACK);
            else bao("Kết thúc – " + (ai == null ? "không ai" : ai) + " thắng", new Color(0x909090), Color.WHITE);
        } else {
            batNut(true);
            if (danDau && !cuaToi) {
                bao("Bạn bị trả giá cao hơn! (" + ai + ")", new Color(0xff6060), Color.WHITE);
                nhatKy.ghi("Bạn bị trả giá cao hơn: " + ai + " " + Chung.tien(gia));
            } else if (cuaToi) bao("Bạn đang dẫn đầu", new Color(0x70d070), Color.BLACK);
            else if (soLan == 0 || thongBao.getText().equals("Chưa kết nối"))
                bao("Phiên đang mở – mời trả giá", new Color(0xe0e0e0), Color.BLACK);
        }
        danDau = cuaToi && !ketThuc;
    }

    @Override
    protected void moiNhip() {
        if (tt == null) return;
        if (Chung.dung(tt.get("ket_thuc"))) {
            nhanDongHo.setText("00:00");
            nhanDongHo.setForeground(new Color(0x606060));
        } else if (hetHan == null) {
            nhanDongHo.setText("∞");
            nhanDongHo.setForeground(Color.BLACK);
        } else {
            double con = Math.max(0, hetHan - Chung.bayGio());
            nhanDongHo.setText(dongHo(con));
            nhanDongHo.setForeground(con < 10 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        }
        nhanGiaHan.setText(Chung.bayGio() < hanBaoGiaHan ? "GIA HẠN!" : "");
    }

    @Override
    protected String dongDinhKy() {
        String con = hetHan == null ? "∞" : Chung.f("%.1f s", Math.max(0, hetHan - Chung.bayGio()));
        Object ai = tt.get("nguoi_dan_dau");
        return "giá " + Chung.tien(Chung.so(tt.get("gia"))) + ", dẫn đầu " + (ai == null ? "–" : ai) + ", còn " + con + ", "
                + (Chung.dung(tt.get("ket_thuc")) ? "KẾT THÚC" : "đang đấu") + "; bạn trả " + soTra + ", nhận " + soNhan;
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43702;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new DauGiaGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
