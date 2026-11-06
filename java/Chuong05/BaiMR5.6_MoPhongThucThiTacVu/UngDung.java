import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.FontMetrics;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.awt.Rectangle;
import java.awt.RenderingHints;
import java.awt.Robot;
import java.awt.Stroke;
import java.awt.TexturePaint;
import java.awt.geom.Line2D;
import java.awt.geom.Rectangle2D;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JComboBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JSlider;
import javax.swing.JSpinner;
import javax.swing.JTable;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingConstants;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.table.DefaultTableCellRenderer;
import javax.swing.table.DefaultTableModel;

/**
 * Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ: ứng dụng giao diện (Swing).
 *
 * Chọn chế độ (đơn luồng/đa luồng × đồng bộ/bất đồng bộ) hoặc "So sánh 4 chế độ"; biểu đồ Gantt chạy theo
 * thời gian: mỗi hàng là một luồng (luồng hệ điều hành, vòng lặp sự kiện hay luồng thợ), màu là tác vụ đang
 * dùng CPU; ô nhạt có gạch chéo: luồng bị chặn chờ I/O; ô viền đứt: sẵn sàng nhưng chờ lõi CPU; ô chấm: chờ
 * join. Hàng "I/O chờ" (bất đồng bộ) vẽ các lần chờ I/O không giữ luồng nào. Nút "Chen ngang U ngay" đưa việc
 * khẩn U vào đúng thời điểm đang xem và mô phỏng lại (lịch sử trước đó không đổi vì bộ lập lịch tất định).
 *
 * Toàn bộ chạy trên luồng giao diện (EDT): mô phỏng rất nhanh (vài mili giây), hoạt hình dùng
 * javax.swing.Timer 30 ms – không có luồng nền nào, không bao giờ chặn vòng lặp sự kiện của Swing.
 *
 * Chạy:  java -cp out UngDung            ứng dụng
 *        java -cp out UngDung --demo     kịch bản tự thao tác, chụp cửa sổ bằng java.awt.Robot thành
 *                                        ket_qua_gui.png (Linux không màn hình: xvfb-run -a java -cp out UngDung --demo)
 */
public class UngDung {
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);

    static final Map<String, Color> MAU = Map.of("T1", new Color(0x2457c5), "T2", new Color(0xe07b00),
            "T3", new Color(0x1b9e5a), "U", new Color(0xd1344b));
    static final Map<String, Color> NHAT = Map.of("T1", new Color(0xc9d8f6), "T2", new Color(0xfbdcb8),
            "T3", new Color(0xc3ead3), "U", new Color(0xf6c5cc));
    static final Color NEN = Color.WHITE, LUOI = new Color(0xe4e7ec), CHU = new Color(0x1d2330),
            XAM = new Color(0x667085), VIEN = new Color(0xc9ced8);
    /** Phông chữ có đủ dấu tiếng Việt, khoảng cách chữ đều; không có thì dùng phông logic Dialog của JDK. */
    static final String PHONG = java.util.Arrays.asList(java.awt.GraphicsEnvironment.getLocalGraphicsEnvironment()
            .getAvailableFontFamilyNames()).contains("DejaVu Sans") ? "DejaVu Sans" : Font.DIALOG;
    static final Font F_THUONG = new Font(PHONG, Font.PLAIN, 12);
    static final Font F_DAM = new Font(PHONG, Font.BOLD, 13);
    static final Font F_NHAN = new Font(PHONG, Font.BOLD, 11);
    static final Font F_NHO = new Font(PHONG, Font.PLAIN, 10);

    /** Một kết quả đang hiển thị: chế độ và kết quả mô phỏng. */
    record HienThi(String cheDo, LapLich.KetQua kq) { }

    final JFrame frame = new JFrame("Mô phỏng thực thi tác vụ – MR5.6");
    List<HienThi> ketQua = new ArrayList<>();
    double T = 0.0;                   // thời điểm đang xem (nhịp, số thực để hoạt hình mượt)
    boolean dangChay = false;
    long last = -1;                   // System.nanoTime() lần nhịp trước, -1 = chưa có
    boolean dangDatThanh = false;     // đang tự đặt thanh kéo (không coi là người dùng kéo)
    final AtomicInteger soLanVe = new AtomicInteger();

    final JTextField fT1 = new JTextField("4", 4), fT2 = new JTextField("3", 4), fT3 = new JTextField("2", 4);
    final JTextField fLat = new JTextField("0.5", 4), fTKhan = new JTextField("0.3", 4);
    final JSpinner spLoi = new JSpinner(new SpinnerNumberModel(2, 1, 4, 1));
    final JComboBox<String> cbTocDo = new JComboBox<>(new String[] {"× 0.25", "× 0.5", "× 1", "× 2", "× 4"});
    final JCheckBox cbPhuThuoc = new JCheckBox("T3 chờ T1, T2 (join/await)", true);
    final JCheckBox cbChia = new JCheckBox("Chia CPU cuối T1 trên 2 luồng", true);
    final JCheckBox cbKhan = new JCheckBox("Việc khẩn U lúc (s)", true);
    final JButton btnChen = new JButton("⚡ Chen ngang U ngay"), btnChay = new JButton("⏸ Dừng");
    final JLabel lblGio = new JLabel("");
    final KhungVe cv = new KhungVe();
    final JSlider thanh = new JSlider(0, 1000, 0);
    final String[] cot = {"Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát"};
    final DefaultTableModel bang = new DefaultTableModel(cot, 0) {
        @Override public boolean isCellEditable(int r, int c) { return false; }
    };
    final JTextArea nk = new JTextArea(30, 40);

    UngDung() {
        javax.swing.plaf.FontUIResource f = new javax.swing.plaf.FontUIResource(PHONG, Font.PLAIN, 11);
        for (Object k : java.util.Collections.list(javax.swing.UIManager.getDefaults().keys())) {
            if (javax.swing.UIManager.get(k) instanceof javax.swing.plaf.FontUIResource) javax.swing.UIManager.put(k, f);
        }
        taoGiaoDien();
        Font kyHieu = new Font(Font.DIALOG, Font.PLAIN, 12);   // phông logic: tự tìm phông khác cho ⚡ ⏸ ▶
        btnChen.setFont(kyHieu);
        btnChay.setFont(kyHieu);
        ve();
        new Timer(30, e -> nhip()).start();
    }

    // ------------------------------------------------------------------ giao diện
    static GridBagConstraints o(int x, int y) {
        GridBagConstraints c = new GridBagConstraints();
        c.gridx = x;
        c.gridy = y;
        c.insets = new Insets(3, 3, 3, 3);
        c.fill = GridBagConstraints.HORIZONTAL;
        c.anchor = GridBagConstraints.WEST;
        return c;
    }

    void taoGiaoDien() {
        JPanel tren = new JPanel(new GridBagLayout());
        tren.setBorder(BorderFactory.createEmptyBorder(6, 6, 6, 6));
        JPanel dk = new JPanel(new GridBagLayout());
        dk.setBorder(BorderFactory.createTitledBorder("Điều khiển"));
        String[][] nut = {{"Đơn luồng – đồng bộ", "don_dong_bo"}, {"Đa luồng – đồng bộ", "da_dong_bo"},
                {"Đơn luồng – bất đồng bộ", "don_bat_dong_bo"}, {"Đa luồng – bất đồng bộ", "da_bat_dong_bo"}};
        for (int i = 0; i < nut.length; i++) {
            String m = nut[i][1];
            JButton b = new JButton(nut[i][0]);
            b.setPreferredSize(new Dimension(172, 28));
            b.setMargin(new Insets(2, 4, 2, 4));
            b.addActionListener(e -> chon(List.of(m)));
            dk.add(b, o(i % 2, i / 2));
        }
        JButton btnSoSanh = new JButton("So sánh 4 chế độ");
        btnSoSanh.addActionListener(e -> chon(new ArrayList<>(LapLich.CHE_DO.keySet())));
        dk.add(btnSoSanh, o(2, 0));
        JButton btnXoa = new JButton("Xóa");
        btnXoa.addActionListener(e -> xoa());
        dk.add(btnXoa, o(2, 1));
        btnChen.addActionListener(e -> chenNgang());
        dk.add(btnChen, o(3, 0));
        btnChay.addActionListener(e -> chayDung());
        dk.add(btnChay, o(3, 1));
        GridBagConstraints c = o(0, 0);
        c.fill = GridBagConstraints.VERTICAL;
        c.insets = new Insets(0, 0, 0, 8);
        tren.add(dk, c);

        JPanel ts = new JPanel(new GridBagLayout());
        ts.setBorder(BorderFactory.createTitledBorder("Thông số"));
        JTextField[] fT = {fT1, fT2, fT3};
        for (int i = 0; i < 3; i++) {
            ts.add(new JLabel("T" + (i + 1) + " (s)"), o(0, i));
            ts.add(fT[i], o(1, i));
        }
        c = o(2, 0);
        c.insets = new Insets(1, 12, 1, 3);
        ts.add(new JLabel("Số lõi CPU"), c);
        ts.add(spLoi, o(3, 0));
        c = o(2, 1);
        c.insets = new Insets(1, 12, 1, 3);
        ts.add(new JLabel("Lát thời gian (s)"), c);
        ts.add(fLat, o(3, 1));
        c = o(2, 2);
        c.insets = new Insets(1, 12, 1, 3);
        ts.add(new JLabel("Tốc độ"), c);
        cbTocDo.setSelectedItem("× 1");
        ts.add(cbTocDo, o(3, 2));
        JCheckBox[] cb = {cbPhuThuoc, cbChia, cbKhan};
        for (int i = 0; i < 3; i++) {
            c = o(4, i);
            c.insets = new Insets(1, 12, 1, 3);
            ts.add(cb[i], c);
        }
        ts.add(fTKhan, o(5, 2));
        c = o(1, 0);
        c.fill = GridBagConstraints.VERTICAL;
        tren.add(ts, c);
        for (JTextField f : new JTextField[] {fT1, fT2, fT3, fLat, fTKhan}) f.setMinimumSize(f.getPreferredSize());
        spLoi.setMinimumSize(spLoi.getPreferredSize());
        cbTocDo.setMinimumSize(cbTocDo.getPreferredSize());
        c = o(2, 0);
        c.weightx = 1;
        tren.add(new JPanel(), c);                          // phần trống bên phải
        lblGio.setFont(new Font(PHONG, Font.BOLD, 14));
        lblGio.setHorizontalAlignment(SwingConstants.CENTER);
        lblGio.setBorder(BorderFactory.createEmptyBorder(2, 0, 6, 0));
        frame.add(tren, BorderLayout.NORTH);

        JPanel giua = new JPanel(new BorderLayout(6, 0));
        giua.setBorder(BorderFactory.createEmptyBorder(0, 6, 6, 6));
        JPanel trai = new JPanel(new BorderLayout(0, 4));
        cv.setBackground(NEN);
        cv.setBorder(BorderFactory.createLineBorder(VIEN));
        trai.add(cv, BorderLayout.CENTER);
        JPanel duoi = new JPanel(new BorderLayout(0, 4));
        thanh.addChangeListener(e -> keo(thanh.getValue()));
        duoi.add(thanh, BorderLayout.NORTH);
        JTable tbl = new JTable(bang);
        tbl.setRowHeight(20);
        tbl.getTableHeader().setFont(tbl.getTableHeader().getFont().deriveFont(Font.BOLD));
        DefaultTableCellRenderer giuaO = new DefaultTableCellRenderer();
        giuaO.setHorizontalAlignment(SwingConstants.CENTER);
        for (int i = 0; i < cot.length; i++) {
            tbl.getColumnModel().getColumn(i).setCellRenderer(giuaO);
            tbl.getColumnModel().getColumn(i).setPreferredWidth(i == 0 ? 150 : 80);
        }
        JScrollPane sp = new JScrollPane(tbl);
        sp.setPreferredSize(new Dimension(400, tbl.getRowHeight() * 4 + 28));
        duoi.add(sp, BorderLayout.CENTER);
        trai.add(duoi, BorderLayout.SOUTH);
        giua.add(trai, BorderLayout.CENTER);
        JPanel phai = new JPanel(new BorderLayout());
        phai.setBorder(BorderFactory.createTitledBorder("Nhật ký sự kiện"));
        nk.setFont(new Font(PHONG, Font.PLAIN, 11));
        nk.setLineWrap(true);
        nk.setWrapStyleWord(true);
        nk.setEditable(false);
        phai.add(new JScrollPane(nk), BorderLayout.CENTER);
        JPanel cotPhai = new JPanel(new BorderLayout());      // đồng hồ mô phỏng trên nhật ký
        cotPhai.add(lblGio, BorderLayout.NORTH);
        cotPhai.add(phai, BorderLayout.CENTER);
        giua.add(cotPhai, BorderLayout.EAST);
        frame.add(giua, BorderLayout.CENTER);
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.setSize(1280, 860);
    }

    // ------------------------------------------------------------------ thao tác
    static double so(JTextField f, double mac) {
        try {
            return Double.parseDouble(f.getText().replace(",", "."));
        } catch (NumberFormatException e) {
            return mac;
        }
    }

    LapLich.ThamSo docThamSo() {
        int loi = Math.min(4, Math.max(1, (Integer) spLoi.getValue()));
        return new LapLich.ThamSo(Math.max(0.4, so(fT1, 4)), Math.max(0.4, so(fT2, 3)), Math.max(0.4, so(fT3, 2)),
                loi, Math.max(0.1, so(fLat, 0.5)), cbPhuThuoc.isSelected(), cbChia.isSelected(),
                cbKhan.isSelected(), Math.max(0.0, so(fTKhan, 0.3)));
    }

    void chon(List<String> dsCheDo) {
        LapLich.ThamSo ts = docThamSo();
        ketQua = new ArrayList<>();
        for (String m : dsCheDo) ketQua.add(new HienThi(m, LapLich.chay(m, ts)));
        T = 0.0;
        dangChay = true;
        last = -1;
        btnChay.setText("⏸ Dừng");
        capNhatBang();
        ve();
    }

    void xoa() {
        ketQua = new ArrayList<>();
        T = 0.0;
        dangChay = false;
        bang.setRowCount(0);
        ve();
    }

    void chayDung() {
        dangChay = !dangChay;
        last = -1;
        btnChay.setText(dangChay ? "⏸ Dừng" : "▶ Chạy");
    }

    /** Đặt việc khẩn U đến ngay thời điểm đang xem rồi mô phỏng lại; giữ nguyên thời điểm xem. */
    void chenNgang() {
        if (ketQua.isEmpty()) return;
        cbKhan.setSelected(true);
        fTKhan.setText(LapLich.giay((int) T));
        double t = T;
        List<String> ds = new ArrayList<>();
        for (HienThi h : ketQua) ds.add(h.cheDo());
        LapLich.ThamSo ts = docThamSo();
        ketQua = new ArrayList<>();
        for (String m : ds) ketQua.add(new HienThi(m, LapLich.chay(m, ts)));
        T = t;
        capNhatBang();
        ve();
    }

    void keo(int giaTri) {
        int D = doDai();
        if (D > 0 && !dangChay && !dangDatThanh) {
            T = giaTri / 1000.0 * D;
            ve();
        }
    }

    int doDai() {
        int d = 0;
        for (HienThi h : ketQua) d = Math.max(d, h.kq().tong);
        return d;
    }

    static String uCho(LapLich.ChiSo s) {
        return s.uCho() >= 0 ? LapLich.giay(s.uCho()) + " s" : "–";
    }

    void capNhatBang() {
        bang.setRowCount(0);
        for (HienThi h : ketQua) {
            LapLich.ChiSo s = LapLich.chiSo(h.kq());
            bang.addRow(new Object[] {LapLich.TEN_NGAN.get(h.cheDo()), LapLich.giay(s.tong()) + " s", s.luong(),
                    s.ban() + " %", uCho(s), s.chen(), s.lat()});
        }
    }

    void nhip() {
        long now = System.nanoTime();
        if (dangChay && !ketQua.isEmpty()) {
            if (last >= 0) {
                double toc = Double.parseDouble(((String) cbTocDo.getSelectedItem()).split(" ")[1]);
                T += (now - last) / 1e9 * 10 * toc;          // × 1: 1 giây mô phỏng mỗi giây thật
                if (T >= doDai()) {
                    T = doDai();
                    dangChay = false;
                    btnChay.setText("▶ Chạy");
                }
            }
            last = now;
            ve();
        }
    }

    /** Cập nhật đồng hồ, thanh kéo, nhật ký rồi yêu cầu vẽ lại biểu đồ. */
    void ve() {
        int D = doDai();
        int t = Math.min((int) T, D);
        lblGio.setText(D > 0 ? "t = " + LapLich.giay(t) + " / " + LapLich.giay(D) + " s" : "Chọn một chế độ");
        if (D > 0) {
            dangDatThanh = true;
            thanh.setValue((int) Math.round(1000 * T / D));
            dangDatThanh = false;
        }
        veNhatKy(t);
        cv.repaint();
    }

    void veNhatKy(int t) {
        StringBuilder sb = new StringBuilder();
        for (HienThi h : ketQua) {
            if (ketQua.size() > 1) sb.append("— ").append(LapLich.TEN_NGAN.get(h.cheDo())).append(" —\n");
            for (LapLich.Ghi g : h.kq().nhatKy) {
                if (g.t() <= t) sb.append(LapLich.phai(LapLich.giay(g.t()), 4)).append(" s  ").append(g.text()).append('\n');
            }
        }
        String s = sb.toString();
        if (!s.equals(nk.getText())) {
            nk.setText(s);
            nk.setCaretPosition(s.length());
        }
    }

    // ------------------------------------------------------------------ vẽ
    /** Vùng vẽ biểu đồ Gantt (tương ứng tk.Canvas của bản Python). */
    @SuppressWarnings("serial")
    final class KhungVe extends JPanel {
        Graphics2D g;
        double X0, X1;
        int D;

        double X(double n) { return X0 + (X1 - X0) * n / Math.max(1, D); }

        void chu(String s, double x, double y, String neo, Font f, Color mau) {
            g.setFont(f);
            g.setColor(mau);
            FontMetrics fm = g.getFontMetrics();
            double w = fm.stringWidth(s);
            double xx = neo.equals("w") ? x : neo.equals("e") ? x - w : x - w / 2;
            g.drawString(s, (float) xx, (float) (y + (fm.getAscent() - fm.getDescent()) / 2.0));
        }

        void hcn(double x0, double y0, double x1, double y1, java.awt.Paint nen, Color vien, float[] net) {
            Rectangle2D r = new Rectangle2D.Double(x0, y0, Math.max(0, x1 - x0), Math.max(0, y1 - y0));
            if (nen != null) {
                g.setPaint(nen);
                g.fill(r);
            }
            if (vien != null) {
                Stroke cu = g.getStroke();
                g.setStroke(net == null ? new BasicStroke(1f)
                        : new BasicStroke(1f, BasicStroke.CAP_BUTT, BasicStroke.JOIN_MITER, 10f, net, 0f));
                g.setColor(vien);
                g.draw(r);
                g.setStroke(cu);
            }
        }

        /** Nền gạch chéo cho ô "bị chặn chờ I/O" (tương ứng stipple gray50 của Tk). */
        TexturePaint gach(String nhan) {
            BufferedImage im = new BufferedImage(6, 6, BufferedImage.TYPE_INT_ARGB);
            Graphics2D gi = im.createGraphics();
            gi.setColor(NHAT.get(nhan));
            gi.fillRect(0, 0, 6, 6);
            Color m = MAU.get(nhan);
            gi.setColor(new Color(m.getRed(), m.getGreen(), m.getBlue(), 70));
            for (int i = 0; i < 6; i++) gi.fillRect(i, 5 - i, 1, 1);
            gi.dispose();
            return new TexturePaint(im, new Rectangle(0, 0, 6, 6));
        }

        @Override
        protected void paintComponent(Graphics g0) {
            super.paintComponent(g0);
            soLanVe.incrementAndGet();
            g = (Graphics2D) g0.create();
            g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
            int W = Math.max(200, getWidth()), H = Math.max(200, getHeight());
            D = doDai();
            int t = Math.min((int) T, D);
            if (ketQua.isEmpty()) {
                chu("Chọn một chế độ để bắt đầu mô phỏng", W / 2.0, H / 2.0, "c", new Font(PHONG, Font.PLAIN, 16), XAM);
                g.dispose();
                return;
            }
            X0 = 110;
            X1 = W - 20;
            double soHang = 0;
            for (HienThi h : ketQua) soHang += h.kq().lan.size() + (h.kq().io.isEmpty() ? 0 : 1) + 1.2;
            double hh = Math.min(48, (H - 50) / soHang);
            // tính trước đáy biểu đồ để vẽ lưới nằm dưới các khối (như tag_lower của Tk)
            double yCuoi = 8;
            for (HienThi h : ketQua)
                yCuoi += hh * 0.9 + hh * (h.kq().lan.size() + (h.kq().io.isEmpty() ? 0 : 1)) + hh * 0.3;
            for (int gg = 0; gg <= D; gg += 5) {
                double x = X(gg);
                g.setColor(gg % 10 != 0 ? LUOI : VIEN);
                g.draw(new Line2D.Double(x, 4, x, yCuoi));
                if (gg % 10 == 0) chu(gg / 10 + " s", x, yCuoi + 10, "c", F_THUONG, XAM);
            }
            double y = 8;
            for (HienThi h : ketQua) {
                chu(LapLich.CHE_DO.get(h.cheDo()), 8, y + hh * 0.4, "w", F_DAM, CHU);
                y += hh * 0.9;
                for (LapLich.Lan l : h.kq().lan) {
                    veLan(l.ten(), l.o(), y, hh, t);
                    y += hh;
                }
                if (!h.kq().io.isEmpty()) {
                    veIo(h.kq(), y, hh, t);
                    y += hh;
                }
                y += hh * 0.3;
            }
            double xt = X(T);
            g.setColor(new Color(0xf2b705));
            g.setStroke(new BasicStroke(2f));
            g.draw(new Line2D.Double(xt, 2, xt, y));
            g.setStroke(new BasicStroke(1f));
            veChuGiai(y + 26, W);
            g.dispose();
        }

        void veLan(String ten, List<LapLich.O> o, double y, double h, int t) {
            chu(ten, 104, y + h / 2, "e", F_THUONG, CHU);
            hcn(X(0), y + 2, X(o.size()), y + h - 2, new Color(0xf6f7f9), null, null);
            int j = 0;
            while (j < Math.min(o.size(), t + 1)) {         // gộp các nhịp liên tiếp cùng trạng thái thành một khối
                int k = j;
                while (k + 1 < o.size() && k + 1 <= t && o.get(k + 1).equals(o.get(j))) k++;
                String tt = o.get(j).tt(), nhan = o.get(j).nhan();
                double x0 = X(j), x1 = k >= t ? X(Math.min(k + 1, T)) : X(k + 1);
                switch (tt) {
                    case LapLich.CPU -> {
                        hcn(x0, y + 3, x1, y + h - 3, MAU.get(nhan), null, null);
                        if (x1 - x0 > 18) chu(nhan, (x0 + x1) / 2, y + h / 2, "c", F_NHAN, Color.WHITE);
                    }
                    case LapLich.IO -> {
                        hcn(x0, y + 6, x1, y + h - 6, gach(nhan), MAU.get(nhan), null);
                        if (x1 - x0 > 40) chu(nhan + " chờ I/O", (x0 + x1) / 2, y + h / 2, "c", F_NHO, CHU);
                    }
                    case LapLich.SAN_SANG -> hcn(x0, y + 5, x1, y + h - 5, null, MAU.get(nhan), new float[] {3, 2});
                    case LapLich.CHO -> {
                        hcn(x0, y + 8, x1, y + h - 8, new Color(0xe9ecef), new Color(0xadb5bd), new float[] {1, 2});
                        if (x1 - x0 > 50) chu("chờ join", (x0 + x1) / 2, y + h / 2, "c", F_NHO, XAM);
                    }
                    default -> { }
                }
                j = k + 1;
            }
        }

        void veIo(LapLich.KetQua kq, double y, double h, int t) {
            chu("I/O chờ", 104, y + h / 2, "e", F_THUONG, CHU);
            hcn(X(0), y + 2, X(kq.tong), y + h - 2, new Color(0xf6fbf3), null, null);
            int nMax = 1;
            for (int n : kq.io) nMax = Math.max(nMax, n);
            for (int j = 0; j < Math.min(kq.io.size(), t + 1); j++) {
                int n = kq.io.get(j);
                if (n != 0) {
                    double c = (h - 6) * n / nMax;
                    hcn(X(j), y + h - 3 - c, X(j + 1), y + h - 3, new Color(0x7fb069), null, null);
                }
            }
        }

        /** Chú giải: mỗi mục một ô mẫu + chữ; vị trí tính theo độ rộng chữ thật để không chồng nhau. */
        void veChuGiai(double y, int W) {
            g.setFont(F_THUONG);
            FontMetrics fm = g.getFontMetrics();
            String[] chu = {"T1 dùng CPU", "T2 dùng CPU", "T3 dùng CPU", "U dùng CPU", "bị chặn chờ I/O",
                    "sẵn sàng, chờ lõi", "chờ join", "I/O bất đồng bộ (không giữ luồng)"};
            double x = 20;
            for (int i = 0; i < chu.length; i++) {
                int w = fm.stringWidth(chu[i]);
                if (x + 23 + w > W - 10) {                 // hết chỗ: xuống hàng
                    x = 20;
                    y += 20;
                }
                switch (i) {
                    case 0, 1, 2, 3 -> hcn(x, y - 6, x + 18, y + 6, MAU.get(i < 3 ? "T" + (i + 1) : "U"), null, null);
                    case 4 -> hcn(x, y - 6, x + 18, y + 6, gach("T1"), MAU.get("T1"), null);
                    case 5 -> hcn(x, y - 6, x + 18, y + 6, null, MAU.get("T1"), new float[] {3, 2});
                    case 6 -> hcn(x, y - 6, x + 18, y + 6, new Color(0xe9ecef), new Color(0xadb5bd), new float[] {1, 2});
                    default -> hcn(x, y - 6, x + 18, y + 6, new Color(0x7fb069), null, null);
                }
                chu(chu[i], x + 23, y, "w", F_THUONG, CHU);
                x += 23 + w + 16;
            }
        }
    }

    // ---------------------------------------------------------------------- kịch bản minh họa
    /** Chạy f trên EDT và chờ kết quả (thao tác điều khiển như người dùng). */
    static <T> T ui(Callable<T> f) throws Exception {
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> {
            try {
                r.set(f.call());
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
        });
        return r.get();
    }

    static void inGui(String s) { OUT.println("[gui] " + s); }

    BufferedImage chup() throws Exception {
        ui(() -> { frame.getRootPane().paintImmediately(frame.getRootPane().getBounds()); return null; });
        Thread.sleep(150);
        return chupKhung(frame);
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

    void demo(String tep) throws Exception {
        Thread.sleep(500);
        inGui("Bấm 'Đa luồng – đồng bộ', dừng ở 3.8 s: U đã chen ngang Luồng T1, "
                + "hai nửa T1 chạy trên Luồng T1 và T1b");
        ui(() -> {
            chon(List.of("da_dong_bo"));
            dangChay = false;
            T = 38.0;
            ve();
            return null;
        });
        Thread.sleep(300);
        BufferedImage a = chup();
        inGui("Bấm 'Đa luồng – bất đồng bộ', chạy hết: T2 chạy tiếp trên thợ khác sau I/O");
        ui(() -> {
            chon(List.of("da_bat_dong_bo"));
            cbTocDo.setSelectedItem("× 4");
            return null;
        });
        long t0 = System.nanoTime();
        int n0 = soLanVe.get();
        while (ui(() -> dangChay) && System.nanoTime() - t0 < 10_000_000_000L) Thread.sleep(50);
        inGui(String.format(Locale.ROOT, "Hoạt hình chạy xong sau %.1f s thật, vẽ lại %d lần (giao diện không đứng)",
                (System.nanoTime() - t0) / 1e9, soLanVe.get() - n0));
        inGui("Bấm 'So sánh 4 chế độ', tua tới cuối");
        ui(() -> {
            chon(new ArrayList<>(LapLich.CHE_DO.keySet()));
            dangChay = false;
            T = doDai();
            ve();
            return null;
        });
        Thread.sleep(300);
        BufferedImage b = chup();
        for (HienThi h : ui(() -> List.copyOf(ketQua))) {
            LapLich.ChiSo s = LapLich.chiSo(h.kq());
            inGui("  " + LapLich.trai(LapLich.TEN_NGAN.get(h.cheDo()), 14) + " hoàn tất " + LapLich.giay(s.tong())
                    + " s, luồng " + s.luong() + ", U chờ " + uCho(s) + ", chen ngang " + s.chen());
        }
        inGui("Bỏ việc khẩn, bấm 'Đa luồng – đồng bộ', dừng ở 3.3 s rồi bấm 'Chen ngang U ngay'");
        LapLich.ChiSo s = ui(() -> {
            cbKhan.setSelected(false);
            chon(List.of("da_dong_bo"));
            dangChay = false;
            T = 33.0;
            btnChen.doClick(0);
            return LapLich.chiSo(ketQua.get(0).kq());
        });
        inGui("  U đến 3.3 s, chờ " + LapLich.giay(s.uCho()) + " s, chen ngang " + s.chen() + " lần, hoàn tất "
                + LapLich.giay(s.tong()) + " s");
        // ghép hai ảnh như "convert a b -bordercolor #d0d4dc -border 4 -append" của bản Python
        int w = Math.max(a.getWidth(), b.getWidth()) + 8;
        BufferedImage hai = new BufferedImage(w, a.getHeight() + b.getHeight() + 16, BufferedImage.TYPE_INT_RGB);
        Graphics2D g = hai.createGraphics();
        g.setColor(new Color(0xd0d4dc));
        g.fillRect(0, 0, hai.getWidth(), hai.getHeight());
        g.drawImage(a, 4, 4, null);
        g.drawImage(b, 4, a.getHeight() + 12, null);
        g.dispose();
        ImageIO.write(hai, "png", new File(tep));
        inGui("Đã chụp cửa sổ: " + tep);
        ui(() -> { frame.dispose(); return null; });
    }

    public static void main(String[] args) throws Exception {
        AtomicReference<UngDung> app = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> {
            app.set(new UngDung());
            app.get().frame.setLocation(0, 0);
            app.get().frame.setVisible(true);
        });
        if (args.length > 0 && args[0].equals("--demo")) {
            app.get().demo("ket_qua_gui.png");
            System.exit(0);
        }
    }
}
