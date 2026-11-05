import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.GridLayout;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.io.IOException;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.function.BiConsumer;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JComboBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JSpinner;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.ListSelectionModel;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.UIManager;
import javax.swing.plaf.FontUIResource;

/**
 * Bài mở rộng MR6.3 – Ứng dụng giao diện (Swing) cho server hội thoại video: "Camera Server".
 *
 * Lưới 4×4 ô: mỗi client một ô (tên, fps nhận được, chấm đỏ khi camera tắt). Khung phải: cổng, danh sách client,
 * điều khiển client đang chọn (Tắt/Bật camera, fps gửi tối đa 1–15, độ phân giải gửi 1/1, 1/2, 1/4), fps chung,
 * nhật ký, START SERVER/STOP SERVER. Các luồng của ServerVideo nhận, ghép mảnh và giải nén; bộ hẹn giờ Swing
 * (~15 lần/giây, trên EDT) chụp nhanh khung mới nhất của từng client và vẽ lại ô. Thao tác của người dùng chạy
 * trên luồng lệnh nền (giữ khóa server, gửi tin TCP), không chạy trên EDT.
 *
 * Chạy: java -cp out ServerVideoGui [host] [cổng_tcp] [cổng_udp]
 */
public class ServerVideoGui {
    static final int SO_O = 16;
    static final DateTimeFormatter GIO = DateTimeFormatter.ofPattern("HH:mm:ss");

    final JFrame frame = new JFrame("Camera Server");
    final JTextField txtIp, txtTcp, txtUdp;
    final JButton btnStart = new JButton("START SERVER"), btnStop = new JButton("STOP SERVER");
    final KhungHinh[] o = new KhungHinh[SO_O];
    final DefaultListModel<String> mdlKhach = new DefaultListModel<>();
    final JList<String> lstKhach = new JList<>(mdlKhach);
    final JButton btnCamera = new JButton("Tắt camera");
    final JSpinner spnFps = new JSpinner(new SpinnerNumberModel(Video.FPS_MAC_DINH, 1, 15, 1));
    final JComboBox<String> cboChia = new JComboBox<>(new String[] {"1/1", "1/2", "1/4"});
    final JSpinner spnFpsChung = new JSpinner(new SpinnerNumberModel(Video.FPS_MAC_DINH, 1, 30, 1));
    final JLabel lblChon = new JLabel("Client đang chọn: (chưa chọn)");
    final JTextArea txtLog = new JTextArea(12, 26);
    final JLabel lblTrangThai = new JLabel("Trạng thái: Server chưa chạy");
    final Timer hen = new Timer(66, e -> capNhat());
    final ExecutorService lenhNen = Executors.newSingleThreadExecutor(r -> Thread.ofPlatform().daemon().name("lenh-video").unstarted(r));

    volatile ServerVideo server;
    final List<Integer> maTheoO = new ArrayList<>();                 // ô i hiển thị client mã maTheoO.get(i)
    final byte[][] daVe = new byte[SO_O][];                           // khung đã vẽ ở mỗi ô (tránh đổi ảnh thừa)
    int maChon = -1;                                                 // client đang chọn (chỉ dùng trên EDT)
    boolean dangDongBo;                                              // đang đặt điều khiển theo client: bỏ qua sự kiện

    static void caiDatGiaoDien() {
        try { UIManager.setLookAndFeel(UIManager.getCrossPlatformLookAndFeelClassName()); } catch (Exception ignored) { }
        FontUIResource f = new FontUIResource(Font.DIALOG, Font.PLAIN, 13);
        for (Object k : new ArrayList<>(UIManager.getLookAndFeelDefaults().keySet()))
            if (UIManager.get(k) instanceof FontUIResource) UIManager.put(k, f);
    }

    static JPanel hang(java.awt.Component... cs) {
        JPanel p = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 2));
        for (java.awt.Component c : cs) p.add(c);
        p.setAlignmentX(0);
        p.setMaximumSize(new Dimension(Integer.MAX_VALUE, p.getPreferredSize().height));   // BoxLayout không kéo giãn hàng
        return p;
    }

    ServerVideoGui(String host, int tcp, int udp) {
        txtIp = new JTextField(host, 9);
        txtTcp = new JTextField(String.valueOf(tcp), 5);
        txtUdp = new JTextField(String.valueOf(udp), 5);
        JPanel luoi = new JPanel(new GridLayout(4, 4, 4, 4));
        luoi.setBorder(BorderFactory.createEmptyBorder(6, 6, 6, 6));
        for (int i = 0; i < SO_O; i++) {
            o[i] = new KhungHinh(232, 174);
            int chiSo = i;
            o[i].addMouseListener(new MouseAdapter() {               // bấm vào ô = chọn client đó
                @Override public void mouseClicked(MouseEvent e) { if (chiSo < maTheoO.size()) lstKhach.setSelectedIndex(chiSo); }
            });
            luoi.add(o[i]);
        }

        JPanel phai = new JPanel();
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        phai.setBorder(BorderFactory.createEmptyBorder(6, 2, 6, 6));
        phai.setPreferredSize(new Dimension(330, 700));
        phai.add(hang(new JLabel("IP:"), txtIp, new JLabel("TCP:"), txtTcp));
        phai.add(hang(new JLabel("UDP:"), txtUdp));
        JLabel tieuDe = new JLabel("Danh sách Clients / Log:");
        tieuDe.setAlignmentX(0);
        phai.add(tieuDe);
        lstKhach.setSelectionMode(ListSelectionModel.SINGLE_SELECTION);
        lstKhach.setVisibleRowCount(5);
        JScrollPane cuonDs = new JScrollPane(lstKhach);
        cuonDs.setAlignmentX(0);
        phai.add(cuonDs);
        lblChon.setAlignmentX(0);
        lblChon.setFont(lblChon.getFont().deriveFont(Font.BOLD));
        phai.add(lblChon);
        phai.add(hang(btnCamera, new JLabel("fps gửi tối đa:"), spnFps));
        phai.add(hang(new JLabel("Độ phân giải gửi:"), cboChia));
        phai.add(hang(new JLabel("fps chung (mọi người nhận):"), spnFpsChung));
        txtLog.setEditable(false);
        txtLog.setLineWrap(true);
        txtLog.setWrapStyleWord(true);
        txtLog.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 11));
        JScrollPane cuonLog = new JScrollPane(txtLog);
        cuonLog.setAlignmentX(0);
        phai.add(cuonLog);
        JPanel nut = new JPanel(new GridLayout(2, 1, 4, 4));
        nut.setAlignmentX(0);
        nut.add(btnStart);
        nut.add(btnStop);
        nut.setMaximumSize(new Dimension(330, 64));
        phai.add(nut);

        lblTrangThai.setBorder(BorderFactory.createEmptyBorder(3, 8, 3, 8));
        frame.getContentPane().add(luoi, BorderLayout.CENTER);
        frame.getContentPane().add(phai, BorderLayout.EAST);
        frame.getContentPane().add(lblTrangThai, BorderLayout.SOUTH);
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.pack();

        btnStop.setEnabled(false);
        datDieuKhien(false);
        btnStart.addActionListener(e -> batDau());
        btnStop.addActionListener(e -> dung());
        lstKhach.addListSelectionListener(e -> {
            int i = lstKhach.getSelectedIndex();
            maChon = i >= 0 && i < maTheoO.size() ? maTheoO.get(i) : -1;
            dongBoDieuKhien();
        });
        btnCamera.addActionListener(e -> {
            boolean tat = btnCamera.getText().startsWith("Tắt");
            lenhKhach((s, k) -> s.tatBatCamera(List.of(k), tat, k.ten));
        });
        spnFps.addChangeListener(e -> {
            int n = (Integer) spnFps.getValue();
            if (!dangDongBo) lenhKhach((s, k) -> { if (k.fpsToiDa != n) s.datFpsToiDa(List.of(k), k.ten, n); });
        });
        cboChia.addActionListener(e -> {
            int n = 1 << cboChia.getSelectedIndex();                   // 1, 2, 4
            if (!dangDongBo) lenhKhach((s, k) -> { if (k.chia != n) s.datDoPhanGiai(List.of(k), k.ten, n); });
        });
        spnFpsChung.addChangeListener(e -> {
            int n = (Integer) spnFpsChung.getValue();
            ServerVideo s = server;
            if (s != null) lenhNen.submit(() -> { synchronized (s.khoa) { if (s.fpsChung != n) s.datFpsChung(n); } });
        });
        hen.start();
    }

    void datDieuKhien(boolean coChon) {
        btnCamera.setEnabled(coChon);
        spnFps.setEnabled(coChon);
        cboChia.setEnabled(coChon);
    }

    /** Ghi nhật ký – gọi được từ mọi luồng. */
    void ghiLog(String s) {
        String dong = "[" + LocalTime.now().format(GIO) + "] " + s + "\n";
        SwingUtilities.invokeLater(() -> {
            txtLog.append(dong);
            if (txtLog.getLineCount() > 500) {
                try { txtLog.replaceRange("", 0, txtLog.getLineEndOffset(99)); } catch (javax.swing.text.BadLocationException ignored) { }
            }
            txtLog.setCaretPosition(txtLog.getDocument().getLength());
        });
    }

    /** Thao tác trên client đang chọn, chạy ở luồng lệnh nền, giữ khóa server. */
    void lenhKhach(BiConsumer<ServerVideo, ServerVideo.Khach> f) {
        ServerVideo s = server;
        int ma = maChon;
        if (s == null || ma < 0) return;
        lenhNen.submit(() -> {
            synchronized (s.khoa) {
                ServerVideo.Khach k = s.dsKhach.get(ma);
                if (k != null) f.accept(s, k);
            }
        });
    }

    void batDau() {
        String host = txtIp.getText().strip();
        int tcp, udp;
        try {
            tcp = Integer.parseInt(txtTcp.getText().strip());
            udp = Integer.parseInt(txtUdp.getText().strip());
        } catch (NumberFormatException e) {
            lblTrangThai.setText("Trạng thái: cổng không hợp lệ");
            return;
        }
        btnStart.setEnabled(false);
        int fpsChung = (Integer) spnFpsChung.getValue();
        lenhNen.submit(() -> {
            ServerVideo s = new ServerVideo(this::ghiLog);
            s.fpsChung = fpsChung;
            try {
                s.batDau(host, tcp, udp);
                server = s;
                ghiLog("Server đã được khởi động");
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Trạng thái: Server đang chạy tại " + host + ":" + tcp + " (UDP " + udp + ")");
                    btnStop.setEnabled(true);
                });
            } catch (IOException e) {
                ghiLog("Không khởi động được: " + e.getMessage());
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Trạng thái: không khởi động được – " + e.getMessage());
                    btnStart.setEnabled(true);
                });
            }
        });
    }

    void dung() {
        ServerVideo s = server;
        if (s == null) return;
        server = null;
        btnStop.setEnabled(false);
        lenhNen.submit(() -> {
            s.dung();
            ghiLog("Server đã dừng");
            SwingUtilities.invokeLater(() -> {
                lblTrangThai.setText("Trạng thái: Server đã dừng");
                btnStart.setEnabled(true);
            });
        });
    }

    record AnhKhach(int ma, String ten, String lyDo, boolean camMo, boolean serverTat, int fpsToiDa, int chia,
                    double fpsVao, double kbpsVao, double fpsRa, byte[] anh, int w, int h) {}

    List<AnhKhach> chupNhanh() {
        ServerVideo s = server;
        List<AnhKhach> ds = new ArrayList<>();
        if (s == null) return ds;
        synchronized (s.khoa) {
            for (ServerVideo.Khach k : s.dsKhach.values())
                ds.add(new AnhKhach(k.ma, k.ten, k.lyDo(), k.camMo(), k.serverTat, k.fpsToiDa, k.chia, k.fpsVao, k.kbpsVao, k.fpsRa,
                        k.anhMoi, k.wMoi, k.hMoi));
        }
        return ds;
    }

    /** Bộ hẹn giờ (EDT): vẽ lại lưới, danh sách client, thanh trạng thái. */
    void capNhat() {
        List<AnhKhach> ds = chupNhanh();
        List<Integer> maMoi = ds.stream().map(AnhKhach::ma).toList();
        if (!maMoi.equals(maTheoO)) {                                  // client vào/ra: dựng lại danh sách
            maTheoO.clear();
            maTheoO.addAll(maMoi);
            int chon = maChon;
            mdlKhach.clear();
            for (AnhKhach a : ds) mdlKhach.addElement("");
            lstKhach.setSelectedIndex(maTheoO.indexOf(chon));
        }
        double tongKbps = 0;
        for (int i = 0; i < SO_O; i++) {
            if (i >= ds.size()) {
                o[i].datAnh(null);
                o[i].datChu("", "", "", false);
                o[i].datChon(false);
                daVe[i] = null;
                continue;
            }
            AnhKhach a = ds.get(i);
            tongKbps += a.kbpsVao();
            if (a.anh() != daVe[i]) {                                  // có khung mới
                daVe[i] = a.anh();
                o[i].datAnh(a.anh() == null ? null : KhungHinh.sangAnh(a.anh(), a.w(), a.h()));
            }
            String tren = "#" + a.ma() + " " + a.ten() + (a.camMo() ? String.format(Locale.ROOT, "  %.0f fps", a.fpsVao()) : "");
            String duoi = String.format(Locale.ROOT, "tối đa %d fps, 1/%d, %.0f kbit/s", a.fpsToiDa(), a.chia(), a.kbpsVao());
            o[i].datChu(tren, a.camMo() ? (a.anh() == null ? "đang chờ hình…" : "") : "Camera " + a.lyDo(), duoi, !a.camMo());
            o[i].datChon(a.ma() == maChon);
            String muc = String.format(Locale.ROOT, "#%d %s – camera %s – vào %.0f fps %.0f kbit/s", a.ma(), a.ten(), a.lyDo(), a.fpsVao(), a.kbpsVao());
            if (!muc.equals(mdlKhach.get(i))) mdlKhach.set(i, muc);
        }
        dongBoDieuKhien(ds);
        if (server != null)
            lblTrangThai.setText(String.format(Locale.ROOT, "Trạng thái: Server đang chạy tại %s:%s – %d client – tổng vào %.0f kbit/s",
                    txtIp.getText(), txtTcp.getText(), ds.size(), tongKbps));
    }

    void dongBoDieuKhien() { dongBoDieuKhien(chupNhanh()); }

    /** Đặt nút/ô chọn theo client đang chọn (không phát lệnh ngược lại server). */
    void dongBoDieuKhien(List<AnhKhach> ds) {
        AnhKhach a = ds.stream().filter(x -> x.ma() == maChon).findFirst().orElse(null);
        datDieuKhien(a != null);
        if (a == null) {
            lblChon.setText("Client đang chọn: (chưa chọn)");
            return;
        }
        dangDongBo = true;
        lblChon.setText("Client đang chọn: #" + a.ma() + " " + a.ten() + " (camera " + a.lyDo() + ")");
        btnCamera.setText(a.serverTat() ? "Bật camera" : "Tắt camera");
        if (!spnFps.getValue().equals(a.fpsToiDa()) && !spnFps.hasFocus()) spnFps.setValue(Math.min(15, a.fpsToiDa()));
        int idx = a.chia() == 4 ? 2 : a.chia() == 2 ? 1 : 0;
        if (cboChia.getSelectedIndex() != idx && !cboChia.isPopupVisible()) cboChia.setSelectedIndex(idx);
        dangDongBo = false;
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        SwingUtilities.invokeLater(() -> {
            caiDatGiaoDien();
            new ServerVideoGui(host, tcp, udp).frame.setVisible(true);
        });
    }
}
