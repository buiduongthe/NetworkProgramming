import java.awt.BasicStroke;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.GridLayout;
import java.awt.geom.AffineTransform;
import java.awt.geom.Arc2D;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Path2D;
import java.awt.geom.Rectangle2D;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.DefaultListCellRenderer;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/**
 * Bài mở rộng MR6.9 – Vòng quay may mắn (ứng dụng người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ vòng
 * quay: bánh xe chia ô (mỗi món quà một ô màu, xen giữa là ô "Chúc may mắn lần sau"), kim chỉ ở đỉnh, đèn viền nhấp
 * nháy khi quay, nút QUAY. Bấm QUAY: yêu cầu {"loai":"quay"} đi tới máy chủ và bánh xe quay tự do ngay; khi có kết
 * quả, bánh xe giảm tốc (ease-out) và dừng đúng ô kết quả (món trúng, hoặc một ô "may mắn lần sau"). Bên phải: số quà
 * còn lại, ảnh từng món (vẽ bằng Graphics2D) với trạng thái còn / đã trao cho ai, danh sách người trúng trực tiếp (từ
 * cap_nhat ≤ 10 lần/giây).
 *
 * Chạy:  java -cp out VongQuayGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43732)
 */
public class VongQuayGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final int[] MAU_QUA = {0xe6194b, 0xf58231, 0xffe119, 0x3cb44b, 0x42d4f4, 0x4363d8, 0x911eb4, 0xf032e6,
            0x9a6324, 0x469990};
    static final String[] TEN_NGAN = {"Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô",
            "Nồi cơm", "Loa", "500k"};
    static final int R = 168;                                      // bán kính bánh xe

    // ===================================================================== ảnh quà (vẽ đơn giản)
    static void oval(Graphics2D g, double x0, double y0, double x1, double y1, Color nen, Color vien, float day) {
        Ellipse2D e = new Ellipse2D.Double(x0, y0, x1 - x0, y1 - y0);
        if (nen != null) { g.setColor(nen); g.fill(e); }
        if (vien != null) { g.setColor(vien); g.setStroke(new BasicStroke(day)); g.draw(e); }
    }

    static void chuNhat(Graphics2D g, double x0, double y0, double x1, double y1, Color nen, Color vien) {
        Rectangle2D r = new Rectangle2D.Double(x0, y0, x1 - x0, y1 - y0);
        if (nen != null) { g.setColor(nen); g.fill(r); }
        if (vien != null) { g.setColor(vien); g.setStroke(new BasicStroke(1)); g.draw(r); }
    }

    static Path2D duong(boolean kin, double... xy) {
        Path2D p = new Path2D.Double();
        p.moveTo(xy[0], xy[1]);
        for (int i = 2; i + 1 < xy.length; i += 2) p.lineTo(xy[i], xy[i + 1]);
        if (kin) p.closePath();
        return p;
    }

    static void net(Graphics2D g, Color mau, float day, double... xy) {
        g.setColor(mau);
        g.setStroke(new BasicStroke(day, BasicStroke.CAP_BUTT, BasicStroke.JOIN_MITER));
        g.draw(duong(false, xy));
    }

    static void cung(Graphics2D g, double x0, double y0, double x1, double y1, double batDau, double rong, Color mau, float day) {
        g.setColor(mau);
        g.setStroke(new BasicStroke(day));
        g.draw(new Arc2D.Double(x0, y0, x1 - x0, y1 - y0, batDau, rong, Arc2D.OPEN));
    }

    /** Vẽ món quà i quanh tâm (cx, cy), nửa cạnh s. xam = true: đã trao (vẽ màu xám). */
    static void veQua(Graphics2D g, int i, double cx, double cy, double s, boolean xam) {
        Color m = xam ? new Color(0x9a9a9a) : new Color(MAU_QUA[i]);
        Color den = xam ? new Color(0x7a7a7a) : new Color(0x303030);
        switch (i) {
            case 0 -> {                                                // xe đạp
                for (double dx : new double[] {-0.55, 0.55})
                    oval(g, cx + (dx - 0.4) * s, cy - 0.1 * s, cx + (dx + 0.4) * s, cy + 0.7 * s, null, den, 2);
                net(g, m, 3, cx - 0.55 * s, cy + 0.3 * s, cx - 0.1 * s, cy - 0.35 * s, cx + 0.4 * s, cy - 0.35 * s,
                        cx + 0.55 * s, cy + 0.3 * s);
                net(g, m, 3, cx - 0.1 * s, cy - 0.35 * s, cx, cy + 0.3 * s, cx + 0.4 * s, cy - 0.35 * s);
                net(g, den, 3, cx - 0.25 * s, cy - 0.5 * s, cx + 0.05 * s, cy - 0.5 * s);
                net(g, den, 3, cx + 0.35 * s, cy - 0.6 * s, cx + 0.55 * s, cy - 0.6 * s);
            }
            case 1 -> {                                                // điện thoại
                chuNhat(g, cx - 0.42 * s, cy - 0.85 * s, cx + 0.42 * s, cy + 0.85 * s, den, den);
                chuNhat(g, cx - 0.34 * s, cy - 0.68 * s, cx + 0.34 * s, cy + 0.6 * s, m, null);
                oval(g, cx - 0.07 * s, cy + 0.66 * s, cx + 0.07 * s, cy + 0.8 * s, new Color(0xd0d0d0), null, 1);
            }
            case 2 -> {                                                // laptop
                chuNhat(g, cx - 0.7 * s, cy - 0.65 * s, cx + 0.7 * s, cy + 0.3 * s, den, den);
                chuNhat(g, cx - 0.6 * s, cy - 0.55 * s, cx + 0.6 * s, cy + 0.2 * s, m, null);
                Path2D p = duong(true, cx - 0.7 * s, cy + 0.35 * s, cx + 0.7 * s, cy + 0.35 * s, cx + 0.9 * s, cy + 0.6 * s,
                        cx - 0.9 * s, cy + 0.6 * s);
                g.setColor(new Color(0xb0b0b0));
                g.fill(p);
                g.setColor(den);
                g.setStroke(new BasicStroke(1));
                g.draw(p);
            }
            case 3 -> {                                                // tai nghe
                cung(g, cx - 0.65 * s, cy - 0.75 * s, cx + 0.65 * s, cy + 0.55 * s, 0, 180, den, 3);
                for (double dx : new double[] {-0.65, 0.65})
                    chuNhat(g, cx + (dx - 0.18) * s, cy - 0.1 * s, cx + (dx + 0.18) * s, cy + 0.6 * s, m, den);
            }
            case 4 -> {                                                // đồng hồ
                chuNhat(g, cx - 0.25 * s, cy - 0.9 * s, cx + 0.25 * s, cy + 0.9 * s, new Color(0x6b4226), null);
                oval(g, cx - 0.55 * s, cy - 0.55 * s, cx + 0.55 * s, cy + 0.55 * s, Color.WHITE, m, 3);
                net(g, den, 2, cx, cy, cx, cy - 0.38 * s);
                net(g, den, 2, cx, cy, cx + 0.28 * s, cy + 0.1 * s);
            }
            case 5 -> {                                                // máy ảnh
                chuNhat(g, cx - 0.3 * s, cy - 0.65 * s, cx + 0.15 * s, cy - 0.4 * s, den, null);
                chuNhat(g, cx - 0.85 * s, cy - 0.45 * s, cx + 0.85 * s, cy + 0.6 * s, m, den);
                oval(g, cx - 0.42 * s, cy - 0.33 * s, cx + 0.42 * s, cy + 0.5 * s, den, Color.WHITE, 1);
                oval(g, cx - 0.2 * s, cy - 0.12 * s, cx + 0.2 * s, cy + 0.28 * s, new Color(0x5080c0), null, 1);
            }
            case 6 -> {                                                // ba lô
                cung(g, cx - 0.4 * s, cy - 0.95 * s, cx + 0.4 * s, cy - 0.45 * s, 0, 180, den, 2);
                chuNhat(g, cx - 0.6 * s, cy - 0.7 * s, cx + 0.6 * s, cy + 0.85 * s, m, den);
                chuNhat(g, cx - 0.4 * s, cy + 0.15 * s, cx + 0.4 * s, cy + 0.65 * s, den, null);
            }
            case 7 -> {                                                // nồi cơm điện
                oval(g, cx - 0.8 * s, cy - 0.55 * s, cx + 0.8 * s, cy - 0.05 * s, new Color(0xe0e0e0), den, 1);
                chuNhat(g, cx - 0.8 * s, cy - 0.3 * s, cx + 0.8 * s, cy + 0.7 * s, m, den);
                chuNhat(g, cx - 0.15 * s, cy - 0.75 * s, cx + 0.15 * s, cy - 0.5 * s, den, null);
                chuNhat(g, cx - 0.25 * s, cy + 0.1 * s, cx + 0.25 * s, cy + 0.4 * s, Color.WHITE, null);
            }
            case 8 -> {                                                // loa
                chuNhat(g, cx - 0.55 * s, cy - 0.9 * s, cx + 0.55 * s, cy + 0.9 * s, m, den);
                oval(g, cx - 0.18 * s, cy - 0.72 * s, cx + 0.18 * s, cy - 0.36 * s, den, null, 1);
                oval(g, cx - 0.42 * s, cy - 0.15 * s, cx + 0.42 * s, cy + 0.69 * s, den, Color.WHITE, 1);
            }
            default -> {                                               // phiếu mua hàng 500k
                chuNhat(g, cx - 0.9 * s, cy - 0.5 * s, cx + 0.9 * s, cy + 0.5 * s, m, den);
                g.setColor(Color.WHITE);
                g.setStroke(new BasicStroke(1, BasicStroke.CAP_BUTT, BasicStroke.JOIN_MITER, 10, new float[] {2, 2}, 0));
                g.draw(duong(false, cx + 0.4 * s, cy - 0.5 * s, cx + 0.4 * s, cy + 0.5 * s));
                g.setFont(GiaoDienChung.chu(Math.max(6, (int) (s * 0.38)), true));
                GiaoDienChung.chuGiua(g, "500K", cx - 0.25 * s, cy);
            }
        }
    }

    /** Các ô của bánh xe: mã quà hoặc null ("may mắn lần sau"); quà rải đều, tối thiểu 8 ô. */
    static Integer[] boTriO(int soQua) {
        int n = Math.max(2 * soQua, 8);
        Integer[] o = new Integer[n];
        for (int i = 0; i < soQua; i++) o[(int) Math.round((double) i * n / soQua)] = i;
        return o;
    }

    /** Chữ nhiều dòng căn giữa tại (x, y), xoay gocDo độ ngược chiều kim đồng hồ (như create_text angle= của Tk). */
    static void chuXoay(Graphics2D g, String s, double x, double y, double gocDo) {
        AffineTransform cu = g.getTransform();
        g.translate(x, y);
        g.rotate(-Math.toRadians(gocDo));
        String[] dong = s.split("\n");
        var fm = g.getFontMetrics();
        double h = fm.getHeight();
        for (int i = 0; i < dong.length; i++) GiaoDienChung.chuGiua(g, dong[i], 0, (i - (dong.length - 1) / 2.0) * h);
        g.setTransform(cu);
    }

    // ===================================================================== cửa sổ
    double goc = 90.0;                                             // góc xoay bánh xe (độ, ngược chiều kim đồng hồ)
    String pha;                                                    // null | "tu_do" | "dung"
    Map<String, Object> ttVe;                                      // trạng thái dùng để vẽ bánh xe
    Map<String, Object> choKq;                                     // kết quả chờ đến lúc bánh xe dừng
    Object[] ketQua;                                               // {text, nền, chữ} của thông báo kết quả cuối
    int nhip;
    double tBam, tDung, goc0, gocDich;
    String nhanHien;                                               // tên hiện trong nhật ký demo (thiết bị thứ hai)
    GiaoDienChung.KhungVe cv;
    JButton nutQuay;
    JLabel thongBao, nhanLuat, nhanCon, nhanDaQuay;
    final List<GiaoDienChung.KhungVe> oQua = new ArrayList<>();
    final List<JLabel> nhanQua = new ArrayList<>();
    volatile List<Object> quaVe = List.of();                       // trạng thái các món để vẽ ô quà
    String daVeQua;
    DefaultListModel<String> dsTrung;
    final List<Boolean> cuaToi = new ArrayList<>();
    Timer hoatHinh;

    VongQuayGui(String ten, String host, int cong) {
        super("Vòng quay may mắn", ten, host, cong);
        nhanHien = ten;
        xay();
    }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new BoxLayout(khung, BoxLayout.X_AXIS));
        JPanel trai = GiaoDienChung.cot();
        trai.add(Box.createVerticalStrut(4));
        cv = new GiaoDienChung.KhungVe(2 * R + 40, 2 * R + 44, this::veBanhXe);
        trai.add(cv);
        nutQuay = GiaoDienChung.nut("QUAY", new Color(0xd62828), 150);
        nutQuay.setFont(GiaoDienChung.chu(16, true));
        nutQuay.setForeground(Color.WHITE);
        nutQuay.setPreferredSize(new Dimension(150, 46));
        nutQuay.addActionListener(e -> quay());
        thongBao = GiaoDienChung.nhanNen("Chưa kết nối", GiaoDienChung.chu(13, true), new Color(0xe0e0e0), 236, 46);
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        thongBao.putClientProperty("chu", "Chưa kết nối");
        JPanel f = GiaoDienChung.hang(nutQuay, thongBao);
        trai.add(Box.createVerticalStrut(4));
        trai.add(f);
        trai.add(Box.createVerticalGlue());
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(10));

        JPanel phai = GiaoDienChung.cot();
        phai.add(Box.createVerticalStrut(4));
        phai.add(GiaoDienChung.nhan("VÒNG QUAY MAY MẮN", GiaoDienChung.chu(19, true), new Color(0x7a1fa2)));
        nhanLuat = GiaoDienChung.nhan("Mỗi người 1 lượt", GiaoDienChung.chu(12, false), null);
        phai.add(nhanLuat);
        nhanCon = GiaoDienChung.nhan("–", GiaoDienChung.chu(16, true), GiaoDienChung.DO_DAM);
        phai.add(Box.createVerticalStrut(2));
        phai.add(nhanCon);
        nhanDaQuay = GiaoDienChung.nhan("–", GiaoDienChung.chu(12, false), null);
        phai.add(nhanDaQuay);
        JPanel g = GiaoDienChung.khungNhom("Quà tặng");
        g.setLayout(new GridLayout(5, 2, 6, 2));
        for (int i = 0; i < MayChu.TEN_QUA.size(); i++) {
            int ii = i;
            GiaoDienChung.KhungVe o = new GiaoDienChung.KhungVe(36, 36, gr -> veOQua(gr, ii));
            o.setBorder(BorderFactory.createLineBorder(new Color(0x808080)));
            JLabel nh = GiaoDienChung.nhan(MayChu.TEN_QUA.get(i), GiaoDienChung.chu(11, false), null);
            nh.setPreferredSize(new Dimension(118, 34));
            JPanel x = GiaoDienChung.hang(o, nh);
            oQua.add(o);
            nhanQua.add(nh);
            g.add(x);
        }
        g.setAlignmentX(0);
        phai.add(Box.createVerticalStrut(4));
        phai.add(g);
        g = GiaoDienChung.khungNhom("Người trúng thưởng (trực tiếp)");
        g.setLayout(new java.awt.BorderLayout());
        dsTrung = new DefaultListModel<>();
        JList<String> ls = new JList<>(dsTrung);
        ls.setFont(GiaoDienChung.chuDeu(12, false));
        ls.setVisibleRowCount(7);
        ls.setFocusable(false);
        ls.setCellRenderer(new DefaultListCellRenderer() {
            @Override
            public Component getListCellRendererComponent(JList<?> l, Object v, int i, boolean chon, boolean td) {
                super.getListCellRendererComponent(l, v, i, false, false);
                setForeground(i < cuaToi.size() && cuaToi.get(i) ? new Color(0x0040c0) : Color.BLACK);
                return this;
            }
        });
        JScrollPane cuon = new JScrollPane(ls);
        cuon.setPreferredSize(new Dimension(290, 140));
        g.add(cuon);
        g.setAlignmentX(0);
        phai.add(Box.createVerticalStrut(4));
        phai.add(g);
        khung.add(phai);
        nutQuay.setEnabled(false);
        hoatHinh = new Timer(30, e -> khungHinh());
    }

    // ------------------------------------------------------------ bánh xe
    void veBanhXe(Graphics2D g) {
        double cx = R + 20, cy = R + 26;
        g.setColor(new Color(0x2b1055));
        g.fillRect(0, 0, 2 * R + 40, 2 * R + 44);
        Map<String, Object> t = ttVe;
        int soQua = t != null ? (int) Chung.so(t.get("so_qua")) : 10;
        java.util.Map<Long, Boolean> con = new java.util.HashMap<>();
        if (t != null) for (Object q : Chung.list(t.get("qua"))) con.put(Chung.so(Chung.map(q).get("ma")), Chung.dung(Chung.map(q).get("con")));
        Integer[] o = boTriO(soQua);
        double ext = 360.0 / o.length;
        oval(g, cx - R - 14, cy - R - 14, cx + R + 14, cy + R + 14, new Color(0xf4b400), new Color(0xa86b00), 3);
        for (int j = 0; j < o.length; j++) {
            double batDau = goc + j * ext;
            Integer ma = o[j];
            Color mau, mauChu;
            String chu;
            if (ma == null) {
                mau = new Color(j % 2 == 1 ? 0xfff7e6 : 0xffe8f0);
                chu = "May mắn\nlần sau";
                mauChu = new Color(0x7a1fa2);
            } else if (con.getOrDefault((long) ma, true)) {
                mau = new Color(MAU_QUA[ma]);
                chu = TEN_NGAN[ma];
                mauChu = ma != 2 && ma != 4 ? Color.WHITE : new Color(0x202020);
            } else {
                mau = new Color(0xbdbdbd);
                chu = TEN_NGAN[ma] + "\n(đã trao)";
                mauChu = new Color(0x606060);
            }
            Arc2D a = new Arc2D.Double(cx - R, cy - R, 2 * R, 2 * R, batDau, ext, Arc2D.PIE);
            g.setColor(mau);
            g.fill(a);
            g.setColor(Color.WHITE);
            g.setStroke(new BasicStroke(2));
            g.draw(a);
            double giua = Math.toRadians(batDau + ext / 2), r = R * 0.62;
            double x = cx + r * Math.cos(giua), y = cy - r * Math.sin(giua);
            double gocChu = ((batDau + ext / 2) % 360 + 360) % 360;
            if (gocChu > 90 && gocChu < 270) gocChu -= 180;           // chữ ở nửa trái: lật cho dễ đọc
            g.setColor(mauChu);
            g.setFont(GiaoDienChung.chu(ma == null || o.length > 12 ? 11 : 12, true));
            chuXoay(g, chu, x, y, gocChu);
        }
        for (int k = 0; k < 24; k++) {                                // đèn viền: nhấp nháy khi quay
            double a = Math.toRadians(k * 15);
            double x = cx + (R + 7) * Math.cos(a), y = cy - (R + 7) * Math.sin(a);
            boolean sang = pha != null ? (k + nhip / 3) % 2 == 0 : k % 2 == 0;
            oval(g, x - 4, y - 4, x + 4, y + 4, new Color(sang ? 0xfffde0 : 0xc08000), null, 1);
        }
        oval(g, cx - 30, cy - 30, cx + 30, cy + 30, new Color(0xd62828), Color.WHITE, 3);
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(11, true));
        chuXoay(g, "MAY\nMẮN", cx, cy, 0);
        Path2D kim = duong(true, cx - 15, cy - R - 24, cx + 15, cy - R - 24, cx, cy - R + 12);   // kim chỉ ở đỉnh
        g.setColor(new Color(0xd62828));
        g.fill(kim);
        g.setColor(Color.WHITE);
        g.setStroke(new BasicStroke(2));
        g.draw(kim);
    }

    void veOQua(Graphics2D g, int i) {
        List<Object> ds = quaVe;
        if (i >= ds.size()) return;                                   // chương trình ít quà hơn: ô trống
        boolean con = Chung.dung(Chung.map(ds.get(i)).get("con"));
        g.setColor(con ? Color.WHITE : new Color(0xe8e8e8));
        g.fillRect(0, 0, 36, 36);
        veQua(g, i, 18, 18, 15, !con);
    }

    void quay() {
        if (kn == null || pha != null) return;
        guiYc("quay", Json.obj());                                    // gửi trước, vẽ sau
        ttVe = tt;
        pha = "tu_do";
        tBam = Chung.bayGio();
        choKq = null;
        nutQuay.setEnabled(false);
        nutQuay.setText("ĐANG QUAY");
        bao("Đang quay…", new Color(0xfff3c4), Color.BLACK);
        nhatKy.ghi("Bấm QUAY");
        hoatHinh.start();
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        nhatKy.ghi(Chung.f("  máy chủ trả lời sau %.1f ms: ", rtt) + (Chung.dung(tin.get("ok"))
                ? (Chung.dung(tin.get("trung")) ? "trúng " + tin.get("ten_qua") : "không trúng")
                : "từ chối – " + tin.get("ly_do")));
        choKq = tin;
        if (pha == null) ketThucQuay();                               // (không xảy ra khi bấm nút)
    }

    void batDauDung() {
        Map<String, Object> tin = choKq;
        Integer[] o = boTriO((int) Chung.so(ttVe.get("so_qua")));
        int j = -1;
        if (Chung.dung(tin.get("ok")) && Chung.dung(tin.get("trung")))
            for (int k = 0; k < o.length; k++) if (o[k] != null && o[k] == Chung.so(tin.get("qua"))) j = k;
        if (j < 0) {
            List<Integer> trong = new ArrayList<>();
            for (int k = 0; k < o.length; k++) if (o[k] == null) trong.add(k);
            j = trong.get(ThreadLocalRandom.current().nextInt(trong.size()));
        }
        double ext = 360.0 / o.length;
        double dich = 90 - (j + 0.5) * ext + ThreadLocalRandom.current().nextDouble(-0.3, 0.3) * ext;
        double m = Math.ceil((dich - (goc - 540)) / 360);
        goc0 = goc;
        gocDich = dich - 360 * m;
        tDung = Chung.bayGio();
        pha = "dung";
    }

    void khungHinh() {
        if (pha == null) {
            hoatHinh.stop();
            return;
        }
        nhip++;
        if (pha.equals("tu_do")) {
            goc -= 20;                                                // quay theo chiều kim đồng hồ
            if (choKq != null && Chung.bayGio() - tBam > 0.6) batDauDung();
        } else {
            double t = Math.min(1.0, (Chung.bayGio() - tDung) / 2.6);
            goc = goc0 + (gocDich - goc0) * (1 - Math.pow(1 - t, 3)); // giảm tốc (ease-out)
            if (t >= 1.0) {
                pha = null;
                hoatHinh.stop();
                cv.repaint();
                ketThucQuay();
                return;
            }
        }
        cv.repaint();
    }

    void ketThucQuay() {
        Map<String, Object> tin = choKq;
        boolean ok = Chung.dung(tin.get("ok")), daQuay = "da_quay".equals(tin.get("ma_loi"));
        if (ok && Chung.dung(tin.get("trung")))
            ketQua = new Object[] {"CHÚC MỪNG! Bạn trúng " + tin.get("ten_qua"), new Color(0xffd700), Color.BLACK};
        else if (ok) ketQua = new Object[] {"Chúc bạn may mắn lần sau", new Color(0xd0d0ff), Color.BLACK};
        else if (daQuay) ketQua = new Object[] {"Bạn đã quay rồi – mỗi người 1 lượt", new Color(0xffb347), Color.BLACK};
        else ketQua = new Object[] {"Lỗi: " + (tin.get("ly_do") == null ? "" : tin.get("ly_do")), new Color(0xff6060), Color.WHITE};
        bao((String) ketQua[0], (Color) ketQua[1], (Color) ketQua[2]);
        nhatKy.ghi("Kết quả: " + ketQua[0]);
        nutQuay.setText(ok || daQuay ? "ĐÃ QUAY" : "QUAY");
        nutQuay.setEnabled(!(ok || daQuay));
        if (tt != null) {
            ttVe = tt;
            cv.repaint();
        }
    }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(GiaoDienChung.html(text).replace("<html>", "<html><div style='text-align:center'>")
                .replace("</html>", "</div></html>"));
        thongBao.putClientProperty("chu", text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() { return String.valueOf(thongBao.getClientProperty("chu")); }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        if (pha == null) {
            boolean moiDatLai = ttVe != null && Chung.so(t.get("da_quay")) < Chung.so(ttVe.get("da_quay"));
            ttVe = t;
            cv.repaint();
            if (moiDatLai || ketQua == null) {                        // đặt lại chương trình: được quay lại
                ketQua = null;
                nutQuay.setText("QUAY");
                nutQuay.setEnabled(true);
                bao("Mời bạn bấm QUAY", new Color(0xe0e0e0), Color.BLACK);
            }
        }
        long soQua = Chung.so(t.get("so_qua")), conLai = Chung.so(t.get("con_lai"));
        nhanLuat.setText("Mỗi người 1 lượt · xác suất trúng " + MayChu.chuP(Chung.sod(t.get("p"))) + " mỗi lượt");
        nhanCon.setText("Còn " + conLai + "/" + soQua + " quà");
        nhanCon.setForeground(conLai > 0 ? GiaoDienChung.DO_DAM : new Color(0x606060));
        nhanDaQuay.setText("Đã quay: " + Chung.tien(Chung.so(t.get("da_quay"))).replace(" đ", "") + " lượt · "
                + t.get("so_trung") + " lượt trúng");
        List<Object> qua = Chung.list(t.get("qua"));
        StringBuilder khoa = new StringBuilder();
        for (Object q : qua) khoa.append(Chung.map(q).get("ma")).append(Chung.map(q).get("con")).append(';');
        if (!khoa.toString().equals(daVeQua)) {
            daVeQua = khoa.toString();
            quaVe = qua;
            for (int i = 0; i < oQua.size(); i++) {
                oQua.get(i).setBorder(i < soQua ? BorderFactory.createLineBorder(new Color(0x808080)) : null);
                oQua.get(i).repaint();
            }
        }
        for (int i = 0; i < nhanQua.size(); i++) {
            if (i < soQua) {
                Map<String, Object> q = Chung.map(qua.get(i));
                boolean con = Chung.dung(q.get("con"));
                Object ai = q.get("nguoi");
                nhanQua.get(i).setText(GiaoDienChung.html(q.get("ten") + (con ? "\ncòn" : "\n→ " + (ai == null ? "?" : ai))));
                nhanQua.get(i).setForeground(con ? GiaoDienChung.XANH_DAM : new Color(0xa00000));
            } else nhanQua.get(i).setText("");
        }
        List<String> ls = new ArrayList<>();
        List<Boolean> toi = new ArrayList<>();
        for (Object o : Chung.list(t.get("nguoi_trung"))) {
            Map<String, Object> x = Chung.map(o);
            ls.add(x.get("gio") + " " + Chung.f("%-6s", x.get("nguoi")) + " " + MayChu.TEN_QUA.get((int) Chung.so(x.get("qua"))));
            toi.add(ten.equals(x.get("nguoi")));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < dsTrung.size(); i++) cu.add(dsTrung.get(i));
        if (!cu.equals(ls)) {
            cuaToi.clear();
            cuaToi.addAll(toi);
            dsTrung.clear();
            for (String x : ls) dsTrung.addElement(x);
        }
    }

    @Override
    protected String dongDinhKy() {
        String kq = ketQua != null ? (String) ketQua[0] : "chưa quay";
        return "còn " + tt.get("con_lai") + "/" + tt.get("so_qua") + " quà, đã quay " + tt.get("da_quay") + " lượt; bạn: " + kq;
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43732;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new VongQuayGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
