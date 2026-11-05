import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.io.IOException;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JComboBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JSlider;
import javax.swing.JSplitPane;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.UIManager;
import javax.swing.border.TitledBorder;
import javax.swing.plaf.FontUIResource;

/**
 * Bài mở rộng MR6.2 – Ứng dụng giao diện (Swing) cho server hội thoại âm thanh: "Bảng điều khiển âm thanh".
 *
 * Ba loại luồng:
 *  - luồng giao diện (EDT – Event Dispatch Thread): duy nhất được chạm vào điều khiển Swing;
 *  - luồng lệnh (lenhNen – một luồng nền): thực hiện thao tác của người dùng trên server (khóa mic, đổi độ lợi…,
 *    vì các thao tác này giữ khóa của server và gửi tin TCP – không làm trên EDT);
 *  - các luồng của ServerAmThanh (nhận kết nối, nhận UDP, nhịp trộn 20 ms) và luồng thu micro.
 * Luồng nền báo về giao diện bằng SwingUtilities.invokeLater (nhật ký); số liệu (mức dBFS, danh sách client)
 * được bộ hẹn giờ Swing (javax.swing.Timer, chạy trên EDT) đọc 10 lần/giây.
 *
 * Chạy: java -cp out ServerAmThanhGui [host] [cổng_tcp] [cổng_udp]
 */
public class ServerAmThanhGui {
    static final Color NEN = new Color(0xB0, 0xC4, 0xDE), NEN_O = new Color(0xC6, 0xD4, 0xE8);
    static final Color XANH = new Color(0x8C, 0xE8, 0x8C), DO = new Color(0xF0, 0x80, 0x80);
    static final Color XANH_DUONG = new Color(0xAD, 0xD8, 0xE6), CAM = new Color(0xFF, 0xA5, 0x00);
    static final double TAN_SO_SERVER = 330;      // âm giả lập của micro server
    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    final JFrame frame = new JFrame("Bảng điều khiển âm thanh – Server");
    final JTextField txtIp, txtTcp, txtUdp;
    final JButton btnStart = nut("Start Server", XANH), btnStop = nut("Stop Server", DO);
    final JButton btnMoTatCa = nut("Mở mic tất cả", XANH_DUONG), btnTatTatCa = nut("Tắt mic tất cả", CAM);
    final JSlider sldChung = new JSlider(0, 200, 100);
    final JLabel lblChung = new JLabel("Âm lượng chung: 100%");
    final JComboBox<String> cboMic = new JComboBox<>();
    final JButton btnMicStart = nut("Start Mic", XANH), btnMicStop = nut("Stop Mic", DO);
    final JSlider sldMic = new JSlider(0, 200, 100);
    final JLabel lblMic = new JLabel("Mic Volume: 100%");
    final ThanhMuc mucMic = new ThanhMuc(150);
    final JLabel lblMucMic = new JLabel(ThanhMuc.chu(Double.NEGATIVE_INFINITY));
    final JLabel lblThietBiMic = new JLabel("Mic: tắt");
    final JCheckBox chkNghe = new JCheckBox("Nghe (phát loa server)");
    final ThanhMuc mucNghe = new ThanhMuc(150);
    final JLabel lblMucNghe = new JLabel(ThanhMuc.chu(Double.NEGATIVE_INFINITY));
    final JLabel lblLoa = new JLabel("Loa: tắt");
    final JLabel lblTrangThai = new JLabel("Server chưa chạy");
    final JLabel lblSoClient = new JLabel("Số client: 0");
    final JPanel pnlKhach = new JPanel();
    final JTextArea txtLog = new JTextArea(8, 60);
    final Map<Integer, DongKhach> dsDong = new LinkedHashMap<>();
    final Timer hen = new Timer(100, e -> capNhat());              // 10 lần/giây, chạy trên EDT
    final ExecutorService lenhNen = Executors.newSingleThreadExecutor(r -> Thread.ofPlatform().daemon().name("lenh-server").unstarted(r));

    volatile ServerAmThanh server;                                  // null khi server dừng
    volatile ThietBiAmThanh.NguonKhung nguonMic;                     // micro của server (null: tắt)
    volatile ThietBiAmThanh.LoaPhat loa;                             // loa của server (null: không có/không nghe)
    volatile double heSoMic = 1;
    volatile boolean nghe;                                           // ô "Nghe" đang chọn

    /** Một hàng trong danh sách client: mã/tên, trạng thái mic, thanh đo, Khóa/Gỡ khóa, độ lợi, âm lượng nghe. */
    final class DongKhach extends JPanel {
        final int ma;
        final String ten;
        final JLabel lblTen, lblMicK = new JLabel("Mic: —"), lblDb = new JLabel(ThanhMuc.chu(Double.NEGATIVE_INFINITY));
        final JLabel lblLoi = new JLabel("Độ lợi: 100%"), lblNgheK = new JLabel("Nghe: 100%");
        final ThanhMuc muc = new ThanhMuc(160);
        final JButton btnKhoa = nut("Khóa", DO);
        final JSlider sldLoi = new JSlider(0, 200, 100);
        boolean khoa;

        DongKhach(int ma, String ten, String ip, double doLoi) {
            super(new FlowLayout(FlowLayout.LEFT, 10, 4));
            this.ma = ma;
            this.ten = ten;
            setBackground(NEN_O);
            setBorder(BorderFactory.createMatteBorder(0, 0, 1, 0, new Color(0x90, 0xA4, 0xBE)));
            lblTen = new JLabel("#" + ma + "  " + ten + "  (" + ip + ")");
            lblTen.setFont(lblTen.getFont().deriveFont(Font.BOLD));
            lblTen.setPreferredSize(new Dimension(150, 20));
            lblMicK.setPreferredSize(new Dimension(150, 20));
            lblDb.setPreferredSize(new Dimension(75, 20));
            lblLoi.setPreferredSize(new Dimension(90, 20));
            sldLoi.setValue((int) Math.round(doLoi * 100));
            sldLoi.setPreferredSize(new Dimension(170, 22));
            sldLoi.setOpaque(false);
            lblLoi.setText("Độ lợi: " + sldLoi.getValue() + "%");
            btnKhoa.setPreferredSize(new Dimension(95, 24));
            add(lblTen);
            add(lblMicK);
            add(muc);
            add(lblDb);
            add(btnKhoa);
            add(lblLoi);
            add(sldLoi);
            add(lblNgheK);
            btnKhoa.addActionListener(e -> {
                boolean tat = !khoa;                                // Khóa = server tắt mic; Gỡ khóa = server bật
                lenh(s -> {
                    ServerAmThanh.Khach k = s.dsKhach.get(ma);
                    if (k != null) s.tatBatMic(List.of(k), tat, k.ten);
                });
            });
            sldLoi.addChangeListener(e -> {
                lblLoi.setText("Độ lợi: " + sldLoi.getValue() + "%");
                if (sldLoi.getValueIsAdjusting()) return;           // chỉ gửi khi thả chuột (tránh gửi dồn dập)
                double g = sldLoi.getValue() / 100.0;
                lenh(s -> {
                    ServerAmThanh.Khach k = s.dsKhach.get(ma);
                    if (k != null && k.doLoi != g) s.datDoLoi(List.of(k), k.ten, g);
                });
            });
        }

        void capNhat(String lyDo, boolean serverKhoa, double mucVao, double amLuongNghe) {
            lblMicK.setText("Mic: " + lyDo);
            lblMicK.setForeground(lyDo.equals("mở") ? new Color(0, 110, 0) : new Color(170, 0, 0));
            muc.datMuc(mucVao);
            lblDb.setText(ThanhMuc.chu(mucVao));
            khoa = serverKhoa;
            btnKhoa.setText(serverKhoa ? "Gỡ khóa" : "Khóa");
            btnKhoa.setBackground(serverKhoa ? XANH : DO);
            lblNgheK.setText(String.format(Locale.ROOT, "Nghe: %.0f%%", amLuongNghe * 100));
        }
    }

    static JButton nut(String chu, Color mau) {
        JButton b = new JButton(chu);
        b.setBackground(mau);
        b.setFocusPainted(false);
        return b;
    }

    static JPanel nhom(String tieuDe) {
        JPanel p = new JPanel(new GridBagLayout());
        p.setBackground(NEN);
        TitledBorder b = BorderFactory.createTitledBorder(BorderFactory.createLineBorder(new Color(0x80, 0x94, 0xB0)), tieuDe);
        b.setTitleFont(b.getTitleFont() == null ? new Font(Font.DIALOG, Font.BOLD, 13) : b.getTitleFont().deriveFont(Font.BOLD));
        p.setBorder(BorderFactory.createCompoundBorder(b, BorderFactory.createEmptyBorder(2, 6, 4, 6)));
        return p;
    }

    /** Đặt điều khiển c vào ô (x, y) của GridBagLayout, chiếm w cột. */
    static void dat(JPanel p, Component c, int x, int y, int w) {
        GridBagConstraints g = new GridBagConstraints();
        g.gridx = x;
        g.gridy = y;
        g.gridwidth = w;
        g.insets = new Insets(3, 4, 3, 4);
        g.anchor = GridBagConstraints.WEST;
        g.fill = c instanceof JSlider || c instanceof JComboBox || c instanceof ThanhMuc ? GridBagConstraints.HORIZONTAL : GridBagConstraints.NONE;
        p.add(c, g);
    }

    /** Font chữ chung có đủ dấu tiếng Việt; giao diện Metal (giống nhau trên Windows và Linux, tô màu nút được). */
    static void caiDatGiaoDien() {
        try { UIManager.setLookAndFeel(UIManager.getCrossPlatformLookAndFeelClassName()); } catch (Exception ignored) { }
        FontUIResource f = new FontUIResource(Font.DIALOG, Font.PLAIN, 13);
        for (Object k : new ArrayList<>(UIManager.getLookAndFeelDefaults().keySet()))
            if (UIManager.get(k) instanceof FontUIResource) UIManager.put(k, f);
    }

    ServerAmThanhGui(String host, int congTcp, int congUdp) {
        txtIp = new JTextField(host, 10);
        txtTcp = new JTextField(String.valueOf(congTcp), 5);
        txtUdp = new JTextField(String.valueOf(congUdp), 5);

        // ---- Nhóm Kết nối
        JPanel ketNoi = nhom("Kết nối");
        dat(ketNoi, new JLabel("IP:"), 0, 0, 1);
        dat(ketNoi, txtIp, 1, 0, 3);
        dat(ketNoi, new JLabel("TCP:"), 0, 1, 1);
        dat(ketNoi, txtTcp, 1, 1, 1);
        dat(ketNoi, new JLabel("UDP:"), 2, 1, 1);
        dat(ketNoi, txtUdp, 3, 1, 1);
        JPanel hangNut = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 0));
        hangNut.setOpaque(false);
        hangNut.add(btnStart);
        hangNut.add(btnStop);
        dat(ketNoi, hangNut, 0, 2, 4);

        // ---- Nhóm Điều khiển chung
        JPanel chung = nhom("Điều khiển chung");
        dat(chung, btnMoTatCa, 0, 0, 1);
        dat(chung, btnTatTatCa, 1, 0, 1);
        dat(chung, lblChung, 0, 1, 2);
        sldChung.setOpaque(false);
        dat(chung, sldChung, 0, 2, 2);

        // ---- Nhóm Micro của server
        JPanel micro = nhom("Micro của server");
        for (String t : ThietBiAmThanh.dsMicro()) cboMic.addItem(t);  // chỉ liệt kê, KHÔNG mở thiết bị
        cboMic.addItem(ThietBiAmThanh.tenGiaLap(TAN_SO_SERVER));
        cboMic.setPreferredSize(new Dimension(240, 24));
        dat(micro, cboMic, 0, 0, 3);
        dat(micro, btnMicStart, 0, 1, 1);
        dat(micro, btnMicStop, 1, 1, 1);
        dat(micro, lblThietBiMic, 2, 1, 1);
        dat(micro, lblMic, 0, 2, 1);
        sldMic.setOpaque(false);
        dat(micro, sldMic, 1, 2, 2);
        dat(micro, mucMic, 0, 3, 2);
        dat(micro, lblMucMic, 2, 3, 1);

        // ---- Nhóm Nghe
        JPanel nghe = nhom("Nghe");
        chkNghe.setOpaque(false);
        dat(nghe, chkNghe, 0, 0, 2);
        dat(nghe, lblLoa, 0, 1, 2);
        dat(nghe, mucNghe, 0, 2, 1);
        dat(nghe, lblMucNghe, 1, 2, 1);

        JPanel tren = new JPanel(new FlowLayout(FlowLayout.LEFT, 8, 6));
        tren.setBackground(NEN);
        tren.add(ketNoi);
        tren.add(chung);
        tren.add(micro);
        tren.add(nghe);

        JPanel trangThai = new JPanel();
        trangThai.setLayout(new BoxLayout(trangThai, BoxLayout.Y_AXIS));
        trangThai.setBackground(Color.WHITE);
        trangThai.setBorder(BorderFactory.createEmptyBorder(4, 10, 4, 10));
        lblTrangThai.setFont(lblTrangThai.getFont().deriveFont(Font.BOLD));
        trangThai.add(lblTrangThai);
        trangThai.add(lblSoClient);

        JPanel dau = new JPanel(new BorderLayout());
        dau.add(tren, BorderLayout.CENTER);
        dau.add(trangThai, BorderLayout.SOUTH);

        pnlKhach.setLayout(new BoxLayout(pnlKhach, BoxLayout.Y_AXIS));
        pnlKhach.setBackground(new Color(0xF4, 0xF4, 0xF4));
        JPanel boc = new JPanel(new BorderLayout());               // giữ các hàng ở phía trên
        boc.add(pnlKhach, BorderLayout.NORTH);
        boc.setBackground(pnlKhach.getBackground());
        JScrollPane cuonKhach = new JScrollPane(boc);
        cuonKhach.setBorder(BorderFactory.createTitledBorder("Client trong phòng"));
        txtLog.setEditable(false);
        txtLog.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        JScrollPane cuonLog = new JScrollPane(txtLog);
        cuonLog.setBorder(BorderFactory.createTitledBorder("Nhật ký sự kiện"));
        JSplitPane giua = new JSplitPane(JSplitPane.VERTICAL_SPLIT, cuonKhach, cuonLog);
        giua.setResizeWeight(0.6);

        frame.getContentPane().add(dau, BorderLayout.NORTH);
        frame.getContentPane().add(giua, BorderLayout.CENTER);
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.setSize(1320, 640);

        btnStop.setEnabled(false);
        btnMicStop.setEnabled(false);
        datNutPhong(false);

        // ---- Sự kiện (chạy trên EDT; việc nặng chuyển sang luồng nền)
        btnStart.addActionListener(e -> batDauServer());
        btnStop.addActionListener(e -> dungServer());
        btnMoTatCa.addActionListener(e -> lenh(s -> s.tatBatMic(new ArrayList<>(s.dsKhach.values()), false, "tất cả")));
        btnTatTatCa.addActionListener(e -> lenh(s -> s.tatBatMic(new ArrayList<>(s.dsKhach.values()), true, "tất cả")));
        sldChung.addChangeListener(e -> {
            lblChung.setText("Âm lượng chung: " + sldChung.getValue() + "%");
            if (sldChung.getValueIsAdjusting()) return;
            double g = sldChung.getValue() / 100.0;
            lenh(s -> { if (s.amLuongChung != g) s.datAmLuongChung(g); });
        });
        sldMic.addChangeListener(e -> {
            lblMic.setText("Mic Volume: " + sldMic.getValue() + "%");
            heSoMic = sldMic.getValue() / 100.0;
        });
        btnMicStart.addActionListener(e -> batMic());
        btnMicStop.addActionListener(e -> tatMic());
        chkNghe.addActionListener(e -> datNghe(chkNghe.isSelected()));
        hen.start();
    }

    void datNutPhong(boolean chay) {
        btnMoTatCa.setEnabled(chay);
        btnTatTatCa.setEnabled(chay);
    }

    /** Ghi nhật ký – gọi được từ mọi luồng (tự chuyển sang EDT). */
    void ghiLog(String s) {
        String dong = "[" + LocalTime.now().format(GIO) + "] " + s + "\n";
        System.out.print("[server-gui] " + dong);                    // cũng in ra dòng lệnh (ghi được vào tệp)
        SwingUtilities.invokeLater(() -> {
            txtLog.append(dong);
            if (txtLog.getLineCount() > 500) {                       // giữ tối đa 500 dòng
                try { txtLog.replaceRange("", 0, txtLog.getLineEndOffset(99)); } catch (javax.swing.text.BadLocationException ignored) { }
            }
            txtLog.setCaretPosition(txtLog.getDocument().getLength());
        });
    }

    /** Chạy một thao tác trên server ở luồng lệnh nền, giữ khóa của server. */
    void lenh(java.util.function.Consumer<ServerAmThanh> f) {
        ServerAmThanh s = server;
        if (s == null) return;
        lenhNen.submit(() -> {
            synchronized (s.khoa) { f.accept(s); }
        });
    }

    // ---------------------------------------------------------------- Start/Stop Server
    void batDauServer() {
        String host = txtIp.getText().strip();
        int tcp, udp;
        try {
            tcp = Integer.parseInt(txtTcp.getText().strip());
            udp = Integer.parseInt(txtUdp.getText().strip());
        } catch (NumberFormatException e) {
            lblTrangThai.setText("Cổng không hợp lệ");
            return;
        }
        btnStart.setEnabled(false);
        double chung = sldChung.getValue() / 100.0;                // đọc điều khiển trên EDT, trước khi sang luồng nền
        lenhNen.submit(() -> {
            ServerAmThanh s = new ServerAmThanh(this::ghiLog);
            s.micServer = this::khungMicServer;                     // móc micro server vào bộ trộn
            s.amLuongChung = chung;
            try {
                s.batDau(host, tcp, udp);
                server = s;
                datMocLoa();
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Server đang chạy tại " + host + ":" + tcp + " (UDP " + udp + ")");
                    lblTrangThai.setForeground(new Color(0, 120, 0));
                    btnStop.setEnabled(true);
                    datNutPhong(true);
                });
            } catch (IOException | RuntimeException e) {
                ghiLog("Không khởi động được: " + e.getMessage());
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Không khởi động được: " + e.getMessage());
                    lblTrangThai.setForeground(new Color(170, 0, 0));
                    btnStart.setEnabled(true);
                });
            }
        });
    }

    void dungServer() {
        ServerAmThanh s = server;
        if (s == null) return;
        server = null;
        btnStop.setEnabled(false);
        lenhNen.submit(() -> {
            s.dung();
            ghiLog("Server đã dừng");
            SwingUtilities.invokeLater(() -> {
                lblTrangThai.setText("Server đã dừng");
                lblTrangThai.setForeground(Color.BLACK);
                btnStart.setEnabled(true);
                datNutPhong(false);
            });
        });
    }

    // ---------------------------------------------------------------- Micro, loa của server
    /** Gọi từ nhịp trộn 20 ms (luồng nền): khung micro server đã nhân Mic Volume, hoặc null. */
    short[] khungMicServer() {
        ThietBiAmThanh.NguonKhung n = nguonMic;
        short[] k = n != null ? n.layKhung() : null;
        return k == null ? null : ThietBiAmThanh.nhan(k, heSoMic);
    }

    void batMic() {
        String chon = (String) cboMic.getSelectedItem();
        btnMicStart.setEnabled(false);
        lenhNen.submit(() -> {                                      // mở thiết bị có thể mất vài trăm ms
            ThietBiAmThanh.NguonKhung n = ThietBiAmThanh.moNguon(chon, TAN_SO_SERVER, this::ghiLog);
            nguonMic = n;
            ghiLog("Micro server: " + n.moTa());
            SwingUtilities.invokeLater(() -> {
                lblThietBiMic.setText(n.that() ? "Đang dùng micro THẬT" : "Đang dùng GIẢ LẬP");
                lblThietBiMic.setForeground(n.that() ? new Color(0, 110, 0) : new Color(0, 70, 160));
                btnMicStop.setEnabled(true);
            });
        });
    }

    void tatMic() {
        ThietBiAmThanh.NguonKhung n = nguonMic;
        nguonMic = null;
        if (n != null) lenhNen.submit(n::dong);
        lblThietBiMic.setText("Mic: tắt");
        lblThietBiMic.setForeground(Color.BLACK);
        btnMicStart.setEnabled(true);
        btnMicStop.setEnabled(false);
        ghiLog("Micro server tắt");
    }

    /** Nghe: mở loa (nếu có) và móc vào bộ trộn; không có loa vẫn đo mức tổng tiếng các client. */
    void datNghe(boolean bat) {
        nghe = bat;
        lenhNen.submit(() -> {
            ThietBiAmThanh.LoaPhat cu = loa;
            loa = bat ? ThietBiAmThanh.LoaPhat.mo() : null;
            if (cu != null) cu.dong();
            datMocLoa();
            String chu = !bat ? "Loa: tắt" : loa != null ? "Loa: THẬT – đang phát" : "Không có loa – chỉ đo mức";
            ghiLog(bat ? "Server nghe: " + (loa != null ? "phát ra loa" : "máy không có loa, chỉ đo mức") : "Server thôi nghe");
            SwingUtilities.invokeLater(() -> lblLoa.setText(chu));
        });
    }

    void datMocLoa() {
        ServerAmThanh s = server;
        if (s == null) return;
        s.loaServer = nghe ? mau -> {
            ThietBiAmThanh.LoaPhat l = loa;
            if (l != null) l.phat(mau);
        } : null;
    }

    // ---------------------------------------------------------------- Cập nhật 10 lần/giây (EDT)
    record AnhKhach(int ma, String ten, String ip, String lyDo, boolean khoa, double doLoi, double nghe, double muc) {}

    void capNhat() {
        ServerAmThanh s = server;
        List<AnhKhach> ds = new ArrayList<>();
        if (s != null) {
            synchronized (s.khoa) {                                  // chụp nhanh trạng thái, giữ khóa rất ngắn
                for (ServerAmThanh.Khach k : s.dsKhach.values())
                    ds.add(new AnhKhach(k.ma, k.ten, k.ip, k.lyDo(), k.serverKhoa, k.doLoi, k.amLuongNghe, k.mucVao));
            }
        }
        boolean doiDs = ds.size() != dsDong.size();
        for (AnhKhach a : ds) doiDs |= !dsDong.containsKey(a.ma());
        if (doiDs) {                                                 // có client vào/ra: dựng lại danh sách hàng
            Map<Integer, DongKhach> cu = new LinkedHashMap<>(dsDong);
            dsDong.clear();
            pnlKhach.removeAll();
            for (AnhKhach a : ds) {
                DongKhach d = cu.containsKey(a.ma()) ? cu.get(a.ma()) : new DongKhach(a.ma(), a.ten(), a.ip(), a.doLoi());
                dsDong.put(a.ma(), d);
                pnlKhach.add(d);
            }
            if (ds.isEmpty()) {
                JLabel trong = new JLabel(s == null ? "  (server chưa chạy)" : "  (chưa có client)");
                trong.setBorder(BorderFactory.createEmptyBorder(8, 8, 8, 8));
                pnlKhach.add(trong);
            }
            pnlKhach.revalidate();
            pnlKhach.repaint();
        }
        for (AnhKhach a : ds) dsDong.get(a.ma()).capNhat(a.lyDo(), a.khoa(), a.muc(), a.nghe());
        lblSoClient.setText("Số client: " + ds.size());
        double mm = s != null ? s.mucMicServer : Double.NEGATIVE_INFINITY, mn = s != null ? s.mucLoaServer : Double.NEGATIVE_INFINITY;
        mucMic.datMuc(mm);
        lblMucMic.setText(ThanhMuc.chu(mm));
        mucNghe.datMuc(mn);
        lblMucNghe.setText(ThanhMuc.chu(mn));
    }

    /** Dùng cho kịch bản minh họa: hàng của client có tên ten (null nếu chưa có). */
    DongKhach dong(String ten) {
        for (DongKhach d : dsDong.values()) if (d.ten.equals(ten)) return d;
        return null;
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        SwingUtilities.invokeLater(() -> {
            caiDatGiaoDien();
            new ServerAmThanhGui(host, tcp, udp).frame.setVisible(true);
        });
    }
}
