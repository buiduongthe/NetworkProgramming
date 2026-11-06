import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.awt.Rectangle;
import java.awt.RenderingHints;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;
import javax.imageio.ImageIO;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.ButtonGroup;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JRadioButton;
import javax.swing.JScrollPane;
import javax.swing.JTable;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.UIManager;
import javax.swing.WindowConstants;
import javax.swing.border.TitledBorder;
import javax.swing.plaf.FontUIResource;
import javax.swing.table.DefaultTableCellRenderer;
import javax.swing.table.DefaultTableModel;

/**
 * Bài mở rộng MR6.7 – Thành phần giao diện Swing dùng chung cho nhóm bài tranh chấp MR6.6–MR6.14.
 *
 * Tệp này GIỐNG NHAU ở các bài (chép nguyên). Gồm:
 *   - Tiện ích (như MR6.5): caiDat, khungNhom, hang, nut, nhan, oNhap, KhungVe, NhatKy, ThongKeDinhKy.
 *   - CuaSoServerCoSo: bảng điều khiển máy chủ đủ mọi phần chung – START/STOP, 4 nút chọn chế độ, độ trễ xử lý,
 *     chỉ số trực tiếp (yc/s, thành công, từ chối, xung đột, hàng đợi, độ trễ, SAI PHẠM tô đỏ), khung mô phỏng tải
 *     (Bắn đồng loạt, So sánh 4 chế độ + bảng), nhật ký. Lớp con (ServerGui) chỉ vẽ khung "Tài nguyên":
 *     veTaiNguyen(khung), capNhatTaiNguyen(tt, tk), thamSoDatLai().
 *   - CuaSoNguoiDungCoSo: khung cửa sổ người dùng – kết nối/rời, nhật ký, thanh trạng thái, gửi yêu cầu; lớp con vẽ
 *     phần nghiệp vụ: ve(khung), capNhat(tt), ketQua(...).
 *   - chup, ghep: dùng cho DemoGui (chụp cửa sổ bằng java.awt.Robot dưới Xvfb, ghép ảnh).
 *
 * Quy tắc: CHỈ luồng giao diện (EDT) chạm vào điều khiển. Luồng mạng/mô phỏng đưa việc vào hàng sự kiện bằng
 * SwingUtilities.invokeLater; số liệu đọc định kỳ bằng javax.swing.Timer. Lớp con gọi xay() ở cuối constructor.
 */
final class GiaoDienChung {
    private GiaoDienChung() {}

    static final Color NEN = new Color(0xb0c4de);
    static final Color XANH_LA = new Color(0x90ee90), DO = new Color(0xf08080), XANH_DUONG = new Color(0xadd8e6),
            CAM = new Color(0xffb347), VANG = new Color(0xffd700);
    static final Color DO_DAM = new Color(0xc00000), XANH_DAM = new Color(0x006400);
    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    static Color mau(int rgb) { return new Color(rgb); }

    /** Giao diện Metal (giống nhau trên Windows, Linux) và một cỡ chữ chung. */
    static void caiDat() {
        try { UIManager.setLookAndFeel(UIManager.getCrossPlatformLookAndFeelClassName()); } catch (Exception ignored) { }
        FontUIResource f = new FontUIResource(Font.DIALOG, Font.PLAIN, 12);
        for (Object k : new ArrayList<>(UIManager.getLookAndFeelDefaults().keySet()))
            if (UIManager.get(k) instanceof FontUIResource) UIManager.put(k, f);
    }

    static Font chu(int co, boolean dam) { return new Font(Font.DIALOG, dam ? Font.BOLD : Font.PLAIN, co); }

    static Font chuDeu(int co, boolean dam) { return new Font(Font.MONOSPACED, dam ? Font.BOLD : Font.PLAIN, co); }

    /** Khung nhóm có tiêu đề (như LabelFrame của Tk). */
    static JPanel khungNhom(String tieuDe) {
        JPanel p = new JPanel();
        p.setBackground(NEN);
        TitledBorder b = BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), tieuDe);
        b.setTitleFont(chu(12, true));
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

    static JPanel cot() {
        JPanel p = new JPanel();
        p.setLayout(new BoxLayout(p, BoxLayout.Y_AXIS));
        p.setBackground(NEN);
        p.setAlignmentX(0);
        p.setAlignmentY(0);
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

    static JLabel nhan(String chu, Font f, Color mau) {
        JLabel l = nhan(chu);
        l.setFont(f);
        if (mau != null) l.setForeground(mau);
        return l;
    }

    /** Nhãn có nền (như tk.Label bg=…). */
    static JLabel nhanNen(String chu, Font f, Color nen, int rong, int cao) {
        JLabel l = nhan(chu, f, null);
        l.setOpaque(true);
        l.setBackground(nen);
        l.setBorder(BorderFactory.createEmptyBorder(2, 4, 2, 4));
        if (rong > 0) {
            Dimension d = new Dimension(rong, cao > 0 ? cao : l.getPreferredSize().height + 6);
            l.setPreferredSize(d);
            l.setMaximumSize(d);
        }
        return l;
    }

    static JTextField oNhap(Object giaTri, int rong) { return new JTextField(String.valueOf(giaTri), rong); }

    static JLabel thanhTrangThai(String chu) {
        JLabel l = new JLabel(chu);
        l.setOpaque(true);
        l.setBackground(Color.WHITE);
        l.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLoweredBevelBorder(),
                BorderFactory.createEmptyBorder(1, 4, 1, 4)));
        l.setPreferredSize(new Dimension(10, l.getPreferredSize().height + 4));   // chữ dài không nới cửa sổ
        return l;
    }

    static String ms(Object us) { return us == null ? "–" : Chung.f("%.1f", Chung.sod(us) / 1000); }

    static String html(String s) {
        return "<html>" + s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\n", "<br>") + "</html>";
    }

    /** Ô vẽ (như Canvas của Tk): kích thước cố định, gọi hàm vẽ mỗi lần repaint, khử răng cưa. */
    static final class KhungVe extends JComponent {
        Consumer<Graphics2D> ve;

        KhungVe(int w, int h, Consumer<Graphics2D> ve) {
            this.ve = ve;
            Dimension d = new Dimension(w, h);
            setPreferredSize(d);
            setMinimumSize(d);
            setMaximumSize(d);
            setAlignmentX(0);
        }

        @Override
        protected void paintComponent(Graphics g0) {
            Graphics2D g = (Graphics2D) g0.create();
            g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
            g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
            g.setRenderingHint(RenderingHints.KEY_STROKE_CONTROL, RenderingHints.VALUE_STROKE_PURE);
            if (ve != null) ve.accept(g);
            g.dispose();
        }
    }

    /** Vẽ chữ căn giữa tại (x, y). */
    static void chuGiua(Graphics2D g, String s, double x, double y) {
        var fm = g.getFontMetrics();
        g.drawString(s, (float) (x - fm.stringWidth(s) / 2.0), (float) (y + fm.getAscent() / 2.0 - fm.getDescent() / 2.0));
    }

    /** Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. Chỉ gọi ghi() trên EDT. inRa = true (chạy riêng): cũng in ra
     *  dòng lệnh kèm tên cửa sổ. */
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
            vanBan.setFont(chuDeu(11, false));
            add(new JScrollPane(vanBan), BorderLayout.CENTER);
            setAlignmentX(0);
        }

        void ghi(String text) {
            String gio = LocalTime.now().format(GIO);
            if (inRa) Chung.inTho(gio + " [" + ten + "] " + text);
            vanBan.append(gio + " " + text + "\n");
            if (vanBan.getLineCount() > 500) {
                try { vanBan.replaceRange("", 0, vanBan.getLineEndOffset(0)); } catch (javax.swing.text.BadLocationException ignored) { }
            }
            vanBan.setCaretPosition(vanBan.getDocument().getLength());
        }
    }

    /** Khi chạy riêng (NhatKy.inRa): cứ `chuKy` giây in một dòng số liệu ra dòng lệnh. Gọi từ bộ hẹn giờ trên EDT. */
    static final class ThongKeDinhKy {
        final String ten;
        final double chuKy;
        double han;

        ThongKeDinhKy(String ten) { this.ten = ten; chuKy = 5.0; han = Chung.bayGio() + chuKy; }

        void inNeuDenHan(Supplier<String> taoDong) {
            double bayGio = Chung.bayGio();
            if (!NhatKy.inRa || bayGio < han) return;
            han = bayGio + chuKy;
            Chung.inTho(LocalTime.now().format(GIO) + " [" + ten + "] " + taoDong.get());
        }
    }

    static GridBagConstraints o(int x, int y) {
        GridBagConstraints c = new GridBagConstraints();
        c.gridx = x;
        c.gridy = y;
        c.anchor = GridBagConstraints.WEST;
        c.insets = new Insets(1, 2, 1, 2);
        return c;
    }

    // ===================================================================== bảng điều khiển máy chủ
    abstract static class CuaSoServerCoSo {
        final String TIEU_DE, TEN_TAI_NGUYEN;
        final JFrame frame;
        final Chung.MayChuCoSo lop;
        final String host;
        final int cong, n0, k0, lan0;
        volatile boolean dangMoPhong;
        List<Map<String, Object>> hangSoSanh = new ArrayList<>();
        JButton nutStart, nutStop, nutTre, nutDatLai, nutBan, nutSoSanh;
        final Map<String, JRadioButton> nutCheDo = new TreeMap<>();
        JTextField oIp, oCong, oTre, oN, oK, oLan;
        final Map<String, JLabel> chiSo = new java.util.LinkedHashMap<>();
        JLabel nhanSaiPham, nhanSaiPhamCt, nhanMoPhong, nhanKetLuan, thanhTt;
        DefaultTableModel bang;
        JTable bangSs;
        final List<Long> saiPhamHang = new ArrayList<>();
        NhatKy nhatKy;
        ThongKeDinhKy dinhKy;
        Timer hen;

        CuaSoServerCoSo(String tieuDe, String tenTaiNguyen, Function<Consumer<String>, Chung.MayChuCoSo> taoLop,
                        String host, int cong, int n, int k, int lan) {
            TIEU_DE = tieuDe;
            TEN_TAI_NGUYEN = tenTaiNguyen;
            this.host = host;
            this.cong = cong;
            n0 = n;
            k0 = k;
            lan0 = lan;
            frame = new JFrame(tieuDe + " – bảng điều khiển");
            lop = taoLop.apply(t -> SwingUtilities.invokeLater(() -> nhatKy.ghi(t)));
        }

        // ------------------------------------------------------------ lớp con viết
        protected abstract void veTaiNguyen(JPanel khung);

        protected abstract void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk);

        protected Map<String, Object> thamSoDatLai() { return Map.of(); }

        /** Dựng giao diện – lớp con gọi ở cuối constructor. */
        protected final void xay() {
            frame.setDefaultCloseOperation(WindowConstants.DO_NOTHING_ON_CLOSE);
            frame.addWindowListener(new java.awt.event.WindowAdapter() {
                @Override public void windowClosing(java.awt.event.WindowEvent e) { dong(); }
            });
            JPanel goc = new JPanel(new BorderLayout());
            goc.setBackground(NEN);
            goc.setBorder(BorderFactory.createEmptyBorder(4, 6, 6, 6));
            JPanel than = cot();
            goc.add(than, BorderLayout.CENTER);

            // ---- hàng trên: Kết nối | Cách xử lý tranh chấp | Chỉ số trực tiếp
            JPanel tren = new JPanel();
            tren.setLayout(new BoxLayout(tren, BoxLayout.X_AXIS));
            tren.setBackground(NEN);
            tren.setAlignmentX(0);
            JPanel g = khungNhom("Kết nối");
            g.setLayout(new GridBagLayout());
            g.add(nhan("IP:"), o(0, 0));
            oIp = oNhap(host, 10);
            g.add(oIp, o(1, 0));
            g.add(nhan("Cổng:"), o(0, 1));
            oCong = oNhap(cong, 6);
            g.add(oCong, o(1, 1));
            nutStart = nut("START SERVER", XANH_LA, 150);
            nutStart.addActionListener(e -> batDau());
            GridBagConstraints c = o(0, 2);
            c.gridwidth = 2;
            c.fill = GridBagConstraints.HORIZONTAL;
            c.insets = new Insets(6, 2, 2, 2);
            g.add(nutStart, c);
            nutStop = nut("STOP SERVER", DO, 150);
            nutStop.addActionListener(e -> dung());
            nutStop.setEnabled(false);
            c = o(0, 3);
            c.gridwidth = 2;
            c.fill = GridBagConstraints.HORIZONTAL;
            g.add(nutStop, c);
            c = o(0, 4);
            c.weighty = 1;
            g.add(Box.createVerticalGlue(), c);
            g.setAlignmentY(0);
            tren.add(g);
            tren.add(Box.createHorizontalStrut(6));

            g = khungNhom("Cách xử lý tranh chấp (đổi được khi đang chạy)");
            g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
            ButtonGroup nhom = new ButtonGroup();
            for (String cd : Chung.CHE_DO) {
                JRadioButton r = new JRadioButton(Chung.f("%-11s %s", cd, Chung.TEN_CHE_DO.get(cd)));
                r.setFont(chuDeu(11, false));
                r.setBackground(NEN);
                r.setActionCommand(cd);
                r.setSelected(cd.equals(lop.kho.cheDo));
                r.addActionListener(e -> doiCheDo(cd));
                r.setAlignmentX(0);
                nhom.add(r);
                g.add(r);
                nutCheDo.put(cd, r);
            }
            oTre = oNhap(Chung.g(lop.kho.treMs), 3);
            nutTre = nut("Áp dụng", XANH_DUONG, 0);
            nutTre.addActionListener(e -> doiTre());
            g.add(hang(nhan("Độ trễ xử lý: ngẫu nhiên 0 –"), oTre, nhan("ms"), nutTre));
            g.add(Box.createVerticalGlue());
            g.setAlignmentY(0);
            tren.add(g);
            tren.add(Box.createHorizontalStrut(6));

            g = khungNhom("Chỉ số trực tiếp (từ lần đặt lại)");
            g.setLayout(new GridBagLayout());
            String[][] dong = {{"ket_noi", "Kết nối / người"}, {"yc_s", "Yêu cầu/giây"}, {"so_yc", "Tổng yêu cầu"},
                    {"thanh_cong", "Thành công"}, {"tu_choi", "Từ chối"}, {"xung_dot", "Xung đột / thử lại"},
                    {"hang_doi", "Hàng đợi (lớn nhất)"}, {"tre", "Xử lý p50/p95/p99"}};
            for (int i = 0; i < dong.length; i++) {
                g.add(nhan(dong[i][1] + ":"), o(0, i));
                JLabel l = nhan("–", chu(12, true), null);
                l.setPreferredSize(new Dimension(220, l.getPreferredSize().height));      // rộng cố định
                chiSo.put(dong[i][0], l);
                g.add(l, o(1, i));
            }
            nhanSaiPham = nhanNen("SAI PHẠM: –", chu(15, true), new Color(0xe0e0e0), 250, 26);
            c = o(2, 0);
            c.gridheight = 2;
            c.anchor = GridBagConstraints.NORTHWEST;
            c.insets = new Insets(1, 12, 1, 2);
            g.add(nhanSaiPham, c);
            nhanSaiPhamCt = nhan(" ", chu(11, false), null);
            nhanSaiPhamCt.setVerticalAlignment(JLabel.TOP);
            nhanSaiPhamCt.setPreferredSize(new Dimension(250, 110));
            c = o(2, 2);
            c.gridheight = 6;
            c.anchor = GridBagConstraints.NORTHWEST;
            c.insets = new Insets(4, 12, 1, 2);
            g.add(nhanSaiPhamCt, c);
            g.setAlignmentY(0);
            tren.add(g);
            than.add(tren);
            than.add(Box.createVerticalStrut(4));

            // ---- hàng giữa: Tài nguyên | Mô phỏng tải
            JPanel giua = new JPanel();
            giua.setLayout(new BoxLayout(giua, BoxLayout.X_AXIS));
            giua.setBackground(NEN);
            giua.setAlignmentX(0);
            g = khungNhom(TEN_TAI_NGUYEN);
            g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
            veTaiNguyen(g);
            nutDatLai = nut("Đặt lại", CAM, 100);
            nutDatLai.addActionListener(e -> datLai());
            JPanel f = new JPanel(new FlowLayout(FlowLayout.RIGHT, 0, 2));
            f.setBackground(NEN);
            f.setAlignmentX(0);
            f.add(nutDatLai);
            g.add(Box.createVerticalGlue());
            g.add(f);
            g.setAlignmentY(0);
            giua.add(g);
            giua.add(Box.createHorizontalStrut(6));

            g = khungNhom("Mô phỏng tải (n người ảo trên k kết nối, rào xuất phát)");
            g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
            oN = oNhap(n0, 5);
            oK = oNhap(k0, 4);
            oLan = oNhap(lan0, 2);
            g.add(hang(nhan("n:"), oN, nhan("  k:"), oK, nhan("  lần/người:"), oLan));
            nutBan = nut("Bắn đồng loạt", CAM, 140);
            nutBan.addActionListener(e -> ban());
            nutSoSanh = nut("So sánh 4 chế độ", XANH_DUONG, 160);
            nutSoSanh.addActionListener(e -> soSanh());
            nhanMoPhong = nhan("");
            g.add(hang(nutBan, nutSoSanh, nhanMoPhong));
            bang = new DefaultTableModel(new Object[] {"Chế độ", "Thành công", "Sai phạm", "Xung đột", "yc/s", "p99 ms"}, 0) {
                @Override public boolean isCellEditable(int r, int cc) { return false; }
            };
            bangSs = new JTable(bang);
            bangSs.setRowHeight(18);
            bangSs.setFocusable(false);
            bangSs.setRowSelectionAllowed(false);
            bangSs.getTableHeader().setReorderingAllowed(false);
            int[] rong = {110, 90, 80, 80, 70, 74};
            DefaultTableCellRenderer ve = new DefaultTableCellRenderer() {
                @Override
                public Component getTableCellRendererComponent(JTable t, Object v, boolean chon, boolean tieuDiem, int r, int cc) {
                    super.getTableCellRendererComponent(t, v, false, false, r, cc);
                    boolean sai = r < saiPhamHang.size() && saiPhamHang.get(r) > 0;
                    setBackground(sai ? new Color(0xffc8c8) : new Color(0xd8f5d8));
                    setForeground(sai ? DO_DAM : Color.BLACK);
                    setHorizontalAlignment(cc == 0 ? LEFT : RIGHT);
                    return this;
                }
            };
            for (int i = 0; i < rong.length; i++) {
                bangSs.getColumnModel().getColumn(i).setPreferredWidth(rong[i]);
                bangSs.getColumnModel().getColumn(i).setCellRenderer(ve);
            }
            bangSs.setPreferredScrollableViewportSize(new Dimension(504, 4 * 18));
            JScrollPane cuon = new JScrollPane(bangSs);
            cuon.setAlignmentX(0);
            cuon.getViewport().setBackground(Color.WHITE);
            g.add(cuon);
            nhanKetLuan = nhan(" ", chu(11, false), null);
            g.add(nhanKetLuan);
            g.add(Box.createVerticalGlue());
            g.setAlignmentY(0);
            giua.add(g);
            than.add(giua);
            than.add(Box.createVerticalStrut(4));

            g = khungNhom("Nhật ký");
            g.setLayout(new BorderLayout());
            nhatKy = new NhatKy(6, 100, "server");
            g.add(nhatKy);
            g.setAlignmentX(0);
            than.add(g);
            dinhKy = new ThongKeDinhKy("server");
            thanhTt = thanhTrangThai("Trạng thái: máy chủ chưa chạy");
            JPanel duoi = new JPanel(new BorderLayout());
            duoi.setBackground(NEN);
            duoi.setBorder(BorderFactory.createEmptyBorder(4, 0, 0, 0));
            duoi.add(thanhTt);
            goc.add(duoi, BorderLayout.SOUTH);
            frame.setContentPane(goc);
            frame.setResizable(false);
            frame.pack();
            hen = new Timer(250, e -> capNhat());
            hen.start();
        }

        // ------------------------------------------------------------ nút
        void batDau() {
            try {
                lop.batDau(oIp.getText().strip(), Integer.parseInt(oCong.getText().strip()));
            } catch (IOException | IllegalArgumentException e) {
                nhatKy.ghi("Không khởi động được: " + e.getMessage());
                return;
            }
            nutStart.setEnabled(false);
            nutStop.setEnabled(true);
            oIp.setEnabled(false);
            oCong.setEnabled(false);
        }

        void dung() {
            nutStop.setEnabled(false);
            Chung.chayNen("dung", () -> {                          // chờ luồng nền dừng: không chặn giao diện
                lop.dung();
                SwingUtilities.invokeLater(() -> {
                    nutStart.setEnabled(true);
                    oIp.setEnabled(true);
                    oCong.setEnabled(true);
                });
            });
        }

        void doiCheDo(String cd) {
            Chung.chayNen("che_do", () -> lop.datCheDo(cd));       // có thể chờ hàng đợi cạn
        }

        /** Chọn chế độ như người dùng bấm nút (kịch bản demo). */
        void chonCheDo(String cd) { nutCheDo.get(cd).doClick(0); }

        void doiTre() {
            try {
                lop.datTre(Double.parseDouble(oTre.getText().strip()));
            } catch (IllegalArgumentException e) {
                nhatKy.ghi("Độ trễ không hợp lệ: " + e.getMessage());
            }
        }

        void datLai() {
            try {
                lop.datLai(thamSoDatLai());
            } catch (IllegalArgumentException e) {
                nhatKy.ghi("Không đặt lại được: " + e.getMessage());
            }
        }

        private int[] docMoPhong() {
            int n = Integer.parseInt(oN.getText().strip()), k = Integer.parseInt(oK.getText().strip()),
                    lan = Integer.parseInt(oLan.getText().strip());
            if (n < 1 || k < 1 || lan < 1) throw new IllegalArgumentException("n, k, lần ≥ 1");
            return new int[] {n, Math.min(k, n), lan};
        }

        interface ViecMoPhong { List<Map<String, Object>> chay(String host, int cong, Consumer<String> bao) throws IOException; }

        /** Chạy mô phỏng trên luồng nền (kết nối TCP tới chính máy chủ này như client bên ngoài). */
        private void chayNen(ViecMoPhong viec) {
            if (dangMoPhong || !lop.chay) {
                nhatKy.ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
                return;
            }
            dangMoPhong = true;
            nutBan.setEnabled(false);
            nutSoSanh.setEnabled(false);
            nhanMoPhong.setText("đang chạy…");
            nhanMoPhong.setForeground(DO_DAM);
            String dc = lop.diaChi;
            int i = dc.lastIndexOf(':');
            String h = dc.substring(0, i);
            int cg = Integer.parseInt(dc.substring(i + 1));
            Consumer<String> bao = text -> SwingUtilities.invokeLater(() -> nhatKy.ghi("[mô phỏng] " + text));
            Chung.chayNen("mo_phong", () -> {
                List<Map<String, Object>> kq;
                try {
                    kq = viec.chay(h, cg, bao);
                } catch (Exception e) {
                    bao.accept("lỗi: " + e.getMessage());
                    kq = null;
                }
                List<Map<String, Object>> k2 = kq;
                SwingUtilities.invokeLater(() -> xongMoPhong(k2));
            });
        }

        void ban() {
            int[] a;
            try { a = docMoPhong(); } catch (IllegalArgumentException e) { nhatKy.ghi(e.getMessage()); return; }
            chayNen((h, cg, bao) -> {
                Map<String, Object> r = MoPhong.chayLuot(h, cg, a[0], a[1], a[2], bao, null, false, null);
                return r == null ? List.of() : List.of(r);
            });
        }

        void soSanh() {
            int[] a;
            try { a = docMoPhong(); } catch (IllegalArgumentException e) { nhatKy.ghi(e.getMessage()); return; }
            chayNen((h, cg, bao) -> MoPhong.soSanh(h, cg, a[0], a[1], a[2], bao, null));
        }

        private void xongMoPhong(List<Map<String, Object>> hang) {
            dangMoPhong = false;
            nutBan.setEnabled(true);
            nutSoSanh.setEnabled(true);
            nhanMoPhong.setText("xong");
            nhanMoPhong.setForeground(XANH_DAM);
            if (hang == null || hang.isEmpty()) return;
            if (hang.size() == Chung.CHE_DO.size()) {
                hangSoSanh = new ArrayList<>(hang);
                bang.setRowCount(0);
                saiPhamHang.clear();
            }
            for (Map<String, Object> h : hang) {
                Object p = h.get("p99_ms");
                Object[] gt = {h.get("che_do"), Chung.py(h.get("thanh_cong")), Chung.py(h.get("sai_pham")),
                        Chung.py(h.get("xung_dot")), Chung.py(h.get("yc_s")), p == null ? "–" : Chung.f("%.1f", Chung.sod(p))};
                int r = -1;
                for (int i = 0; i < bang.getRowCount(); i++) if (bang.getValueAt(i, 0).equals(h.get("che_do"))) r = i;
                if (r >= 0) {
                    for (int cc = 0; cc < gt.length; cc++) bang.setValueAt(gt[cc], r, cc);
                    saiPhamHang.set(r, Chung.so(h.get("sai_pham")));
                } else {
                    bang.addRow(gt);
                    saiPhamHang.add(Chung.so(h.get("sai_pham")));
                }
            }
            List<String> sai = new ArrayList<>(), dung = new ArrayList<>();
            for (Map<String, Object> h : hang) (Chung.so(h.get("sai_pham")) > 0 ? sai : dung).add(String.valueOf(h.get("che_do")));
            nhanKetLuan.setText((sai.isEmpty() ? "" : "Sai phạm: " + String.join(", ", sai) + ". ")
                    + (dung.isEmpty() ? "" : "Không sai phạm: " + String.join(", ", dung) + "."));
            bangSs.repaint();
        }

        String ketLuan() { return nhanKetLuan.getText(); }

        // ------------------------------------------------------------ vẽ lại định kỳ (250 ms)
        void capNhat() {
            if (lop.chay) {
                Map<String, Object> tk = lop.thongKeDayDu();
                Map<String, Object> tt = lop.trangThai();
                chiSo.get("ket_noi").setText(tk.get("so_ket_noi") + " / " + tk.get("so_nguoi"));
                chiSo.get("yc_s").setText(tk.get("yc_s") + "  (TB " + tk.get("thong_luong") + ")");
                chiSo.get("so_yc").setText(String.valueOf(tk.get("so_yc")));
                chiSo.get("thanh_cong").setText(String.valueOf(tk.get("thanh_cong")));
                Map<String, Object> ml = Chung.map(tk.get("ma_loi"));
                List<String> p = new ArrayList<>();
                for (Map.Entry<String, Object> e : new TreeMap<>(ml).entrySet()) p.add(e.getKey() + " " + e.getValue());
                chiSo.get("tu_choi").setText(tk.get("tu_choi") + (ml.isEmpty() ? "" : "  (" + String.join(", ", p) + ")"));
                chiSo.get("xung_dot").setText(tk.get("xung_dot") + " / " + tk.get("thu_lai") + " (hết lượt " + tk.get("het_lan_thu") + ")");
                chiSo.get("hang_doi").setText(tk.get("hang_doi") + "  (" + tk.get("hang_doi_max") + ")");
                chiSo.get("tre").setText(ms(tk.get("p50_us")) + " / " + ms(tk.get("p95_us")) + " / " + ms(tk.get("p99_us")) + " ms");
                long sp = Chung.so(tk.get("sai_pham_tong"));
                nhanSaiPham.setText("SAI PHẠM: " + sp);
                nhanSaiPham.setBackground(sp > 0 ? new Color(0xff6060) : new Color(0x70d070));
                nhanSaiPham.setForeground(sp > 0 ? Color.WHITE : Color.BLACK);
                List<String> ct = new ArrayList<>();
                for (Object o : Chung.list(tk.get("sai_pham"))) ct.add(Chung.map(o).get("ten") + " " + Chung.map(o).get("so"));
                StringBuilder s = new StringBuilder(String.join(", ", ct));
                List<Object> vd = Chung.list(tk.get("vi_du_sai_pham"));
                for (int i = 0; i < Math.min(2, vd.size()); i++) s.append("\n• ").append(vd.get(i));
                nhanSaiPhamCt.setText(html(s.toString()));
                String cd = String.valueOf(tk.get("che_do"));
                if (!nutCheDo.get(cd).isSelected() && !dangMoPhong) nutCheDo.get(cd).setSelected(true);   // đổi từ xa
                capNhatTaiNguyen(tt, tk);
                thanhTt.setText("Trạng thái: máy chủ chạy tại " + lop.diaChi + " – chế độ " + cd + ", " + tk.get("so_ket_noi")
                        + " kết nối – " + tk.get("tom_tat"));
                dinhKy.inNeuDenHan(() -> cd + ": " + tk.get("so_yc") + " yc, tc " + tk.get("thanh_cong") + ", " + tk.get("yc_s")
                        + " yc/s, sai phạm " + sp + "; " + tk.get("tom_tat"));
            } else {
                thanhTt.setText("Trạng thái: máy chủ chưa chạy");
            }
        }

        void dong() {
            hen.stop();
            lop.dung();
            frame.dispose();
        }
    }

    // ===================================================================== cửa sổ người dùng
    /** Kết nối/rời, nhật ký, thanh trạng thái; lớp con vẽ phần nghiệp vụ trong khung. */
    abstract static class CuaSoNguoiDungCoSo {
        final String TIEU_DE;
        final JFrame frame;
        String ten;
        final String host0;
        final int cong0;
        KetNoiKhach kn;
        Map<String, Object> tt;                                    // cap_nhat mới nhất
        Double rttCuoi;
        JTextField oTen, oIp, oCong;
        JButton nutKetNoi, nutRoi;
        JPanel khung;
        NhatKy nhatKy;
        ThongKeDinhKy dinhKy;
        JLabel thanhTt;
        Timer hen;
        boolean song = true;

        CuaSoNguoiDungCoSo(String tieuDe, String ten, String host, int cong) {
            TIEU_DE = tieuDe;
            this.ten = ten;
            host0 = host;
            cong0 = cong;
            frame = new JFrame(tieuDe + " – " + ten);
        }

        // ------------------------------------------------------------ lớp con viết
        protected abstract void ve(JPanel khung);

        protected abstract void capNhat(Map<String, Object> tt);

        protected abstract void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt);

        protected void suKien(String text) { nhatKy.ghi("* " + text); }

        /** Gọi mỗi 200 ms trên EDT (đồng hồ đếm ngược…). */
        protected void moiNhip() {}

        protected String dongDinhKy() { return ""; }

        protected final void xay() {
            frame.setDefaultCloseOperation(WindowConstants.DO_NOTHING_ON_CLOSE);
            frame.addWindowListener(new java.awt.event.WindowAdapter() {
                @Override public void windowClosing(java.awt.event.WindowEvent e) { dong(); }
            });
            JPanel goc = new JPanel(new BorderLayout());
            goc.setBackground(NEN);
            goc.setBorder(BorderFactory.createEmptyBorder(4, 6, 6, 6));
            JPanel than = cot();
            goc.add(than, BorderLayout.CENTER);
            oTen = oNhap(ten, 7);
            oIp = oNhap(host0, 9);
            oCong = oNhap(cong0, 5);
            nutKetNoi = nut("Kết nối", XANH_LA, 90);
            nutKetNoi.addActionListener(e -> ketNoi());
            nutRoi = nut("Rời", DO, 60);
            nutRoi.addActionListener(e -> roi());
            nutRoi.setEnabled(false);
            than.add(hang(nhan("Tên:"), oTen, nhan(" IP:"), oIp, nhan(" Cổng:"), oCong, nutKetNoi, nutRoi));
            khung = new JPanel();
            khung.setBackground(NEN);
            khung.setAlignmentX(0);
            ve(khung);
            than.add(khung);
            JPanel g = khungNhom("Nhật ký");
            g.setLayout(new BorderLayout());
            nhatKy = new NhatKy(4, 50, ten);
            g.add(nhatKy);
            g.setAlignmentX(0);
            than.add(g);
            dinhKy = new ThongKeDinhKy(ten);
            thanhTt = thanhTrangThai("Chưa kết nối");
            JPanel duoi = new JPanel(new BorderLayout());
            duoi.setBackground(NEN);
            duoi.setBorder(BorderFactory.createEmptyBorder(4, 0, 0, 0));
            duoi.add(thanhTt);
            goc.add(duoi, BorderLayout.SOUTH);
            frame.setContentPane(goc);
            frame.setResizable(false);
            frame.pack();
            hen = new Timer(200, e -> nhip());
            hen.start();
        }

        // ------------------------------------------------------------ kết nối
        void ketNoi() {
            String t = oTen.getText().strip(), h = oIp.getText().strip();
            int c;
            try {
                c = Integer.parseInt(oCong.getText().strip());
            } catch (NumberFormatException e) {
                nhatKy.ghi("Cổng không hợp lệ");
                return;
            }
            ten = t;
            nutKetNoi.setEnabled(false);
            KetNoiKhach[] ref = new KetNoiKhach[1];
            KetNoiKhach k = new KetNoiKhach(tin -> SwingUtilities.invokeLater(() -> nhanCapNhat(tin)),
                    tin -> SwingUtilities.invokeLater(() -> nhanTin(tin)),
                    () -> SwingUtilities.invokeLater(() -> matKetNoi(ref[0], "Mất kết nối máy chủ")));
            ref[0] = k;
            Chung.chayNen("ket_noi", () -> {                      // kết nối trên luồng nền: không đơ giao diện
                try {
                    k.ketNoi(h, c, t);
                } catch (IOException | RuntimeException e) {
                    SwingUtilities.invokeLater(() -> {
                        nhatKy.ghi("Không kết nối được: " + e.getMessage());
                        nutKetNoi.setEnabled(true);
                    });
                    return;
                }
                SwingUtilities.invokeLater(() -> daKetNoi(k));
            });
        }

        protected void daKetNoi(KetNoiKhach k) {
            kn = k;
            nutRoi.setEnabled(true);
            oTen.setEnabled(false);
            oIp.setEnabled(false);
            oCong.setEnabled(false);
            nhatKy.ghi("Đã kết nối " + k.chao.get("may_chu") + " (chế độ " + k.chao.get("che_do") + ")");
            nhanCapNhat(k.trangThai);
            k.batDauDoc();
        }

        void roi() {
            if (kn != null) {
                KetNoiKhach k = kn;
                kn = null;
                k.khiDong = () -> { };
                k.dong();
                matKetNoi(null, "Đã rời");
            }
        }

        protected void matKetNoi(KetNoiKhach k, String lyDo) {
            if (k != null && k != kn) return;
            kn = null;
            nhatKy.ghi(lyDo);
            thanhTt.setText(lyDo);
            nutKetNoi.setEnabled(true);
            nutRoi.setEnabled(false);
            oTen.setEnabled(true);
            oIp.setEnabled(true);
            oCong.setEnabled(true);
        }

        private void nhanCapNhat(Map<String, Object> t) {
            tt = t;
            capNhat(t);
            capNhatThanhTt();
        }

        private void nhanTin(Map<String, Object> tin) {
            if ("su_kien".equals(tin.get("loai"))) suKien(String.valueOf(tin.get("noi_dung")));
            else if ("loi".equals(tin.get("loai"))) nhatKy.ghi("Lỗi: " + tin.get("ly_do"));
        }

        void capNhatThanhTt() {
            if (kn != null && tt != null) {
                String rtt = rttCuoi == null ? "" : Chung.f(", khứ hồi %.1f ms", rttCuoi);
                thanhTt.setText("Kết nối " + oIp.getText() + ":" + oCong.getText() + " – máy chủ chế độ " + tt.get("che_do") + rtt);
            }
        }

        // ------------------------------------------------------------ gửi yêu cầu
        void guiYc(String loai, Map<String, Object> truong) {
            if (kn == null) {
                nhatKy.ghi("Chưa kết nối");
                return;
            }
            try {
                kn.guiYc(loai, (tin, rtt) -> SwingUtilities.invokeLater(() -> {   // luồng đọc -> EDT
                    rttCuoi = rtt;
                    ketQua(loai, truong, tin, rtt);
                    capNhatThanhTt();
                }), new java.util.LinkedHashMap<>(truong));
            } catch (IOException e) {
                nhatKy.ghi("Gửi lỗi: " + e.getMessage());
            }
        }

        private void nhip() {
            moiNhip();
            if (kn != null && tt != null) dinhKy.inNeuDenHan(this::dongDinhKy);
        }

        void dong() {
            song = false;
            hen.stop();
            if (kn != null) {
                kn.khiDong = () -> { };
                kn.dong();
            }
            frame.dispose();
        }
    }

    // ===================================================================== chụp ảnh (DemoGui)
    /** Chụp một cửa sổ bằng java.awt.Robot (gọi ngoài EDT). */
    static BufferedImage chup(JFrame f) throws Exception {
        return chupKhung(f);
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

    /** Ghép ảnh theo hàng (căn trên), các hàng xếp dọc (căn trái), nền #d0d0d0, viền 4 – như convert +append/-append. */
    static void ghep(List<List<BufferedImage>> hang, String tep) throws IOException {
        int W = 0, H = 0;
        for (List<BufferedImage> h : hang) {
            int w = 0, c = 0;
            for (BufferedImage a : h) { w += a.getWidth(); c = Math.max(c, a.getHeight()); }
            W = Math.max(W, w);
            H += c;
        }
        BufferedImage ra = new BufferedImage(W + 8, H + 8, BufferedImage.TYPE_INT_RGB);
        Graphics2D g = ra.createGraphics();
        g.setColor(new Color(0xd0d0d0));
        g.fillRect(0, 0, ra.getWidth(), ra.getHeight());
        int y = 4;
        for (List<BufferedImage> h : hang) {
            int x = 4, c = 0;
            for (BufferedImage a : h) {
                g.drawImage(a, x, y, null);
                x += a.getWidth();
                c = Math.max(c, a.getHeight());
            }
            y += c;
        }
        g.dispose();
        ImageIO.write(ra, "png", new File(tep));
    }
}
