import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.GridLayout;
import java.io.IOException;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComboBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/**
 * Bài mở rộng MR6.3 – Ứng dụng giao diện (Swing) cho client hội thoại video: "Camera Client".
 *
 * Kết nối server (IP, cổng, tên); Camera (chọn camera thật qua ffmpeg / mẫu thử ffmpeg / "Camera giả lập",
 * START = tự bật, CANCEL = tự tắt); khung xem trước camera của mình; khung "lưới nhận từ server" (chọn fps nhận,
 * độ phân giải nhận 1/1, 1/2); thanh trạng thái (đang truyền, fps gửi, kbit/s). Phần mạng/camera ở PhienVideo
 * (luồng nền); liệt kê camera (chạy ffmpeg, có thể mất 1–2 s) và kết nối chạy trên luồng nền "viecNen".
 *
 * Chạy: java -cp out KhachVideoGui [host] [cổng_tcp] [tên]
 */
public class KhachVideoGui {
    final JFrame frame;
    final JTextField txtIp, txtCong, txtTen;
    final JButton btnKetNoi = new JButton("Kết nối"), btnNgat = new JButton("Ngắt kết nối");
    final JComboBox<String> cboCamera = new JComboBox<>();
    final JButton btnStart = new JButton("START"), btnCancel = new JButton("CANCEL");
    final JLabel lblThietBi = new JLabel("Camera: tắt");
    final KhungHinh xemTruoc = new KhungHinh(240, 180), luoi = new KhungHinh(240, 180);
    final JComboBox<Integer> cboFpsNhan = new JComboBox<>(new Integer[] {1, 2, 5, 10, 15});
    final JComboBox<String> cboChiaNhan = new JComboBox<>(new String[] {"1/1", "1/2"});
    final JLabel lblNhan = new JLabel("Nhận: —");
    final JLabel lblTrangThai = new JLabel("Trạng thái: chưa kết nối");
    final JLabel lblSuKien = new JLabel(" ");
    final Timer hen = new Timer(66, e -> capNhat());
    final ExecutorService viecNen;

    volatile PhienVideo phien;
    PhienVideo.Anh daVeMinh, daVeLuoi;
    String trangThaiRieng;                                         // "Bị từ chối…" (chỉ dùng trên EDT)
    volatile String tenLog = "";

    static JPanel nhom(String tieuDe) {
        JPanel p = new JPanel();
        p.setLayout(new BoxLayout(p, BoxLayout.Y_AXIS));
        p.setBorder(BorderFactory.createTitledBorder(tieuDe));
        return p;
    }

    KhachVideoGui(String host, int cong, String ten) {
        frame = new JFrame("Camera Client – " + ten);
        viecNen = Executors.newSingleThreadExecutor(r -> Thread.ofPlatform().daemon().name("viec-" + ten).unstarted(r));
        txtIp = new JTextField(host, 10);
        txtCong = new JTextField(String.valueOf(cong), 5);
        txtTen = new JTextField(ten, 5);

        JPanel ketNoi = nhom("Kết nối Server");
        ketNoi.add(ServerVideoGui.hang(new JLabel("Server IP:"), txtIp));
        ketNoi.add(ServerVideoGui.hang(new JLabel("Port:"), txtCong, new JLabel("Tên:"), txtTen));
        ketNoi.add(ServerVideoGui.hang(btnKetNoi, btnNgat));
        JPanel camera = nhom("Camera");
        camera.add(ServerVideoGui.hang(new JLabel("Chọn Camera:")));
        cboCamera.addItem(CameraFfmpeg.GIA_LAP);
        cboCamera.setPreferredSize(new Dimension(230, 24));
        camera.add(ServerVideoGui.hang(cboCamera));
        camera.add(ServerVideoGui.hang(btnStart, btnCancel));
        camera.add(ServerVideoGui.hang(lblThietBi));
        JPanel tren = new JPanel(new GridLayout(1, 2, 6, 0));
        tren.add(ketNoi);
        tren.add(camera);

        JPanel trai = new JPanel(new BorderLayout());
        trai.setBorder(BorderFactory.createTitledBorder("Camera của tôi (xem trước)"));
        trai.add(xemTruoc, BorderLayout.CENTER);
        JPanel phai = new JPanel(new BorderLayout());
        phai.setBorder(BorderFactory.createTitledBorder("Lưới nhận từ server"));
        phai.add(luoi, BorderLayout.CENTER);
        cboFpsNhan.setSelectedItem(Video.FPS_MAC_DINH);
        phai.add(ServerVideoGui.hang(new JLabel("fps nhận:"), cboFpsNhan, new JLabel("độ phân giải:"), cboChiaNhan, lblNhan), BorderLayout.SOUTH);
        JPanel giua = new JPanel(new GridLayout(1, 2, 6, 0));
        giua.add(trai);
        giua.add(phai);

        JPanel duoi = new JPanel();
        duoi.setLayout(new BoxLayout(duoi, BoxLayout.Y_AXIS));
        duoi.setBorder(BorderFactory.createEmptyBorder(2, 6, 4, 6));
        lblTrangThai.setFont(lblTrangThai.getFont().deriveFont(Font.BOLD));
        duoi.add(lblTrangThai);
        duoi.add(lblSuKien);

        JPanel noiDung = new JPanel(new BorderLayout(0, 4));
        noiDung.setBorder(BorderFactory.createEmptyBorder(6, 6, 0, 6));
        noiDung.add(tren, BorderLayout.NORTH);
        noiDung.add(giua, BorderLayout.CENTER);
        noiDung.add(duoi, BorderLayout.SOUTH);
        frame.getContentPane().add(noiDung);
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.pack();

        datNut(false);
        xemTruoc.datChu("", "Camera tắt", "", true);
        luoi.datChu("", "Chưa kết nối", "", false);
        btnKetNoi.addActionListener(e -> ketNoi());
        btnNgat.addActionListener(e -> ngat("Trạng thái: đã ngắt kết nối"));
        btnStart.addActionListener(e -> batCamera());
        btnCancel.addActionListener(e -> {
            PhienVideo p = phien;
            if (p != null) viecNen.submit(p::tatCamera);
            btnStart.setEnabled(true);
            btnCancel.setEnabled(false);
            cboCamera.setEnabled(true);
            lblThietBi.setText("Camera: tắt");
            lblThietBi.setForeground(Color.BLACK);
        });
        Runnable chonNhan = () -> {
            PhienVideo p = phien;
            int fps = (Integer) cboFpsNhan.getSelectedItem(), chia = cboChiaNhan.getSelectedIndex() + 1;
            if (p != null) viecNen.submit(() -> p.chonNhan(fps, chia));
        };
        cboFpsNhan.addActionListener(e -> chonNhan.run());
        cboChiaNhan.addActionListener(e -> chonNhan.run());
        viecNen.submit(() -> {                                     // liệt kê camera (chạy ffmpeg) ở luồng nền
            List<String> ds = CameraFfmpeg.dsCamera();
            SwingUtilities.invokeLater(() -> {
                cboCamera.removeAllItems();
                for (String t : ds) cboCamera.addItem(t);
            });
        });
        hen.start();
    }

    void datNut(boolean daKetNoi) {
        btnKetNoi.setEnabled(!daKetNoi);
        btnNgat.setEnabled(daKetNoi);
        for (JTextField t : new JTextField[] {txtIp, txtCong, txtTen}) t.setEnabled(!daKetNoi);
        btnStart.setEnabled(daKetNoi);
        btnCancel.setEnabled(false);
        cboCamera.setEnabled(true);
    }

    void baoTuPhien(String s) {
        System.out.println("[" + tenLog + "] " + s);
        SwingUtilities.invokeLater(() -> {
            lblSuKien.setText(s);
            if (s.startsWith("Bị từ chối")) trangThaiRieng = "Trạng thái: " + s;
            else if (s.startsWith("Camera: mở")) trangThaiRieng = null;
            if (s.contains("chuyển sang camera giả lập")) {
                lblThietBi.setText("Đang dùng: Camera GIẢ LẬP");
                lblThietBi.setForeground(new Color(0, 70, 160));
            }
        });
    }

    void ketNoi() {
        String host = txtIp.getText().strip(), ten = txtTen.getText().strip();
        int cong;
        try {
            cong = Integer.parseInt(txtCong.getText().strip());
        } catch (NumberFormatException e) {
            lblTrangThai.setText("Trạng thái: cổng không hợp lệ");
            return;
        }
        tenLog = ten;
        btnKetNoi.setEnabled(false);
        lblTrangThai.setText("Trạng thái: đang kết nối " + host + ":" + cong + "…");
        int fps = (Integer) cboFpsNhan.getSelectedItem(), chia = cboChiaNhan.getSelectedIndex() + 1;
        viecNen.submit(() -> {
            PhienVideo p = new PhienVideo(ten, this::baoTuPhien, () -> SwingUtilities.invokeLater(() -> ngat("Trạng thái: mất kết nối tới server")));
            try {
                p.ketNoi(host, cong);
                if (fps != Video.FPS_MAC_DINH || chia != 1) p.chonNhan(fps, chia);
                phien = p;
                SwingUtilities.invokeLater(() -> {
                    datNut(true);
                    trangThaiRieng = null;
                });
            } catch (IOException e) {
                p.ngat();
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Trạng thái: không kết nối được – " + e.getMessage());
                    btnKetNoi.setEnabled(true);
                });
            }
        });
    }

    void ngat(String lyDo) {
        PhienVideo p = phien;
        phien = null;
        if (p != null) viecNen.submit(p::ngat);
        datNut(false);
        lblTrangThai.setText(lyDo);
        lblThietBi.setText("Camera: tắt");
        lblThietBi.setForeground(Color.BLACK);
    }

    /** START: mở camera đã chọn trên luồng nền (khởi động ffmpeg) rồi xin server bật camera. */
    void batCamera() {
        PhienVideo p = phien;
        if (p == null) return;
        String chon = (String) cboCamera.getSelectedItem();
        btnStart.setEnabled(false);
        cboCamera.setEnabled(false);
        viecNen.submit(() -> {
            CameraFfmpeg.NguonCamera n = CameraFfmpeg.mo(chon, p.ten, p.ma, this::baoTuPhien);
            p.batCamera(n);
            SwingUtilities.invokeLater(() -> {
                lblThietBi.setText(n.that() ? "Đang dùng: camera THẬT" : n.moTa().equals(CameraFfmpeg.GIA_LAP)
                        ? "Đang dùng: Camera GIẢ LẬP" : "Đang dùng: mẫu thử ffmpeg (testsrc)");
                lblThietBi.setForeground(n.that() ? new Color(0, 110, 0) : new Color(0, 70, 160));
                btnCancel.setEnabled(true);
            });
        });
    }

    /** Bộ hẹn giờ (EDT): vẽ khung mới, cập nhật trạng thái. */
    String thietBiDaIn = "";                                          // chỉ dùng trên EDT
    long demNhip;

    void capNhat() {
        PhienVideo p = phien;
        PhienVideo.Anh m = p != null ? p.khungMinh : null, l = p != null ? p.luoiMoi : null;
        if (m != daVeMinh) {
            daVeMinh = m;
            xemTruoc.datAnh(m == null ? null : KhungHinh.sangAnh(m.rgb(), m.w(), m.h()));
        }
        if (l != daVeLuoi) {
            daVeLuoi = l;
            luoi.datAnh(l == null ? null : KhungHinh.sangAnh(l.rgb(), l.w(), l.h()));
        }
        if (p == null) {
            xemTruoc.datChu("", "Camera tắt", "", true);
            luoi.datChu("", "Chưa kết nối", "", false);
            return;
        }
        boolean gui = !p.tuTat && p.camMoServer && p.nguon != null;
        if (!lblThietBi.getText().equals(thietBiDaIn)) {              // ghi nhật ký khi đổi camera
            thietBiDaIn = lblThietBi.getText();
            System.out.println("[" + tenLog + "] " + thietBiDaIn);
        }
        if (++demNhip % 75 == 0)                                      // 75 × 66 ms ≈ 5 s: thống kê vào nhật ký
            System.out.println(String.format(Locale.ROOT, "[%s] Gửi %.0f fps, %.0f kbit/s%s; nhận %.0f fps, %.0f kbit/s",
                    tenLog, p.fpsGui, p.kbpsGui, gui ? "" : " (không gửi)", p.fpsNhan, p.kbpsNhan));
        xemTruoc.datChu(m == null ? "" : "160x120" + (p.chia > 1 ? " -> gửi 1/" + p.chia : ""), m == null ? "Camera tắt" : "",
                "", !gui);
        luoi.datChu(l == null ? "" : l.w() + "x" + l.h(), l == null ? "Đang chờ lưới…" : "", "", false);
        lblNhan.setText(String.format(Locale.ROOT, "%.0f fps, %.0f kbit/s", p.fpsNhan, p.kbpsNhan));
        String tt;
        if (trangThaiRieng != null && !p.camMoServer) tt = trangThaiRieng;
        else if (gui) tt = String.format(Locale.ROOT, "Trạng thái: Đang truyền dữ liệu camera… gửi %.0f fps, %.0f kbit/s (tối đa %d fps, 1/%d)",
                p.fpsGui, p.kbpsGui, p.fpsServer, p.chia);
        else if (p.tuTat) tt = "Trạng thái: đã kết nối (mã " + p.ma + ") – camera tắt";
        else if (p.lyDo.contains("server")) tt = "Trạng thái: server đang tắt camera của bạn – không gửi";
        else tt = "Trạng thái: đang chờ server cho phép bật camera";
        lblTrangThai.setText(tt);
        lblTrangThai.setForeground(gui ? new Color(0, 120, 0) : tt.contains("từ chối") || tt.contains("server đang tắt") ? new Color(170, 0, 0) : Color.BLACK);
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        String ten = args.length > 2 ? args[2] : "A";
        SwingUtilities.invokeLater(() -> {
            ServerVideoGui.caiDatGiaoDien();
            new KhachVideoGui(host, cong, ten).frame.setVisible(true);
        });
    }
}
