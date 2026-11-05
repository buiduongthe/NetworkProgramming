import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.io.IOException;
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
import javax.swing.JSlider;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/**
 * Bài mở rộng MR6.2 – Ứng dụng giao diện (Swing) cho client hội thoại âm thanh.
 *
 * Cửa sổ chỉ lo hiển thị và nhận thao tác; phần mạng, thu, phát nằm trong PhienAmThanh (luồng nền). Mọi thao tác
 * có thể chờ (kết nối, mở thiết bị) chạy trên luồng nền "viecNen"; kết quả trả về EDT bằng invokeLater. Bộ hẹn giờ
 * Swing 10 lần/giây đọc mức dBFS, trạng thái mic, độ lợi server để cập nhật nhãn và thanh đo.
 *
 * Chạy: java -cp out KhachAmThanhGui [host] [cổng_tcp] [tên] [--tan-so 440]
 */
public class KhachAmThanhGui {
    final JFrame frame;
    final JTextField txtIp, txtCong, txtTen;
    final JButton btnKetNoi = ServerAmThanhGui.nut("Connect", ServerAmThanhGui.XANH);
    final JButton btnNgat = ServerAmThanhGui.nut("Disconnect", ServerAmThanhGui.DO);
    final JSlider sldNghe = new JSlider(0, 200, 100);
    final JLabel lblNghe = new JLabel("Âm lượng nghe: 100%");
    final JButton btnAudioStart = ServerAmThanhGui.nut("Start Audio", ServerAmThanhGui.XANH);
    final JButton btnAudioStop = ServerAmThanhGui.nut("Stop Audio", ServerAmThanhGui.DO);
    final JLabel lblServerVol = new JLabel("Server Vol: --");
    final ThanhMuc mucNhan = new ThanhMuc(220);
    final JLabel lblMucNhan = new JLabel(ThanhMuc.chu(Double.NEGATIVE_INFINITY));
    final JLabel lblLoa = new JLabel("Loa: tắt (chỉ đo mức)");
    final JComboBox<String> cboMic = new JComboBox<>();
    final JButton btnMicStart = ServerAmThanhGui.nut("Start Mic", ServerAmThanhGui.XANH);
    final JButton btnMicStop = ServerAmThanhGui.nut("Stop Mic", ServerAmThanhGui.DO);
    final JSlider sldMic = new JSlider(0, 200, 100);
    final JLabel lblMic = new JLabel("Mic Volume: 100%");
    final ThanhMuc mucMic = new ThanhMuc(220);
    final JLabel lblMucMic = new JLabel(ThanhMuc.chu(Double.NEGATIVE_INFINITY));
    final JLabel lblThietBi = new JLabel("Thiết bị: chưa mở");
    final JLabel lblTrangThai = new JLabel("Chưa kết nối");
    final JLabel lblSuKien = new JLabel(" ");
    final Timer hen = new Timer(100, e -> capNhat());
    final ExecutorService viecNen;
    final double tanSo;

    volatile PhienAmThanh phien;                                    // null khi chưa kết nối
    String trangThaiRieng;                                          // "Bị từ chối…" (chỉ dùng trên EDT)

    KhachAmThanhGui(String host, int cong, String ten, double tanSo) {
        this.tanSo = tanSo;
        frame = new JFrame("Client âm thanh – " + ten);
        viecNen = Executors.newSingleThreadExecutor(r -> Thread.ofPlatform().daemon().name("viec-" + ten).unstarted(r));
        txtIp = new JTextField(host, 11);
        txtCong = new JTextField(String.valueOf(cong), 5);
        txtTen = new JTextField(ten, 6);

        JPanel ketNoi = ServerAmThanhGui.nhom("Kết nối");
        ServerAmThanhGui.dat(ketNoi, new JLabel("IP:"), 0, 0, 1);
        ServerAmThanhGui.dat(ketNoi, txtIp, 1, 0, 1);
        ServerAmThanhGui.dat(ketNoi, new JLabel("Cổng:"), 2, 0, 1);
        ServerAmThanhGui.dat(ketNoi, txtCong, 3, 0, 1);
        ServerAmThanhGui.dat(ketNoi, new JLabel("Tên:"), 0, 1, 1);
        ServerAmThanhGui.dat(ketNoi, txtTen, 1, 1, 1);
        JPanel nutKetNoi = hang(btnKetNoi, btnNgat);
        ServerAmThanhGui.dat(ketNoi, nutKetNoi, 2, 1, 2);

        JPanel nhan = ServerAmThanhGui.nhom("Nhận âm thanh");
        ServerAmThanhGui.dat(nhan, lblNghe, 0, 0, 1);
        sldNghe.setOpaque(false);
        ServerAmThanhGui.dat(nhan, sldNghe, 1, 0, 2);
        ServerAmThanhGui.dat(nhan, hang(btnAudioStart, btnAudioStop), 0, 1, 2);
        lblServerVol.setFont(lblServerVol.getFont().deriveFont(Font.BOLD));
        ServerAmThanhGui.dat(nhan, lblServerVol, 2, 1, 1);
        ServerAmThanhGui.dat(nhan, mucNhan, 0, 2, 2);
        ServerAmThanhGui.dat(nhan, lblMucNhan, 2, 2, 1);
        ServerAmThanhGui.dat(nhan, lblLoa, 0, 3, 3);

        JPanel micro = ServerAmThanhGui.nhom("Micro");
        for (String t : ThietBiAmThanh.dsMicro()) cboMic.addItem(t); // chỉ liệt kê, chưa mở thiết bị
        cboMic.addItem(ThietBiAmThanh.tenGiaLap(tanSo));
        cboMic.setPreferredSize(new Dimension(300, 24));
        ServerAmThanhGui.dat(micro, cboMic, 0, 0, 3);
        ServerAmThanhGui.dat(micro, hang(btnMicStart, btnMicStop), 0, 1, 2);
        ServerAmThanhGui.dat(micro, lblThietBi, 2, 1, 1);
        ServerAmThanhGui.dat(micro, lblMic, 0, 2, 1);
        sldMic.setOpaque(false);
        ServerAmThanhGui.dat(micro, sldMic, 1, 2, 2);
        ServerAmThanhGui.dat(micro, mucMic, 0, 3, 2);
        ServerAmThanhGui.dat(micro, lblMucMic, 2, 3, 1);

        JPanel trangThai = new JPanel();
        trangThai.setLayout(new BoxLayout(trangThai, BoxLayout.Y_AXIS));
        trangThai.setBackground(ServerAmThanhGui.NEN);
        lblTrangThai.setOpaque(true);
        lblTrangThai.setBackground(Color.WHITE);
        lblTrangThai.setBorder(BorderFactory.createEmptyBorder(2, 6, 2, 6));
        lblTrangThai.setFont(lblTrangThai.getFont().deriveFont(Font.BOLD));
        trangThai.add(lblTrangThai);
        trangThai.add(lblSuKien);

        JPanel noiDung = new JPanel();
        noiDung.setLayout(new BoxLayout(noiDung, BoxLayout.Y_AXIS));
        noiDung.setBackground(ServerAmThanhGui.NEN);
        noiDung.setBorder(BorderFactory.createEmptyBorder(8, 10, 8, 10));
        for (JPanel p : new JPanel[] {ketNoi, nhan, micro}) {
            p.setAlignmentX(0);
            noiDung.add(p);
        }
        trangThai.setAlignmentX(0);
        noiDung.add(trangThai);
        frame.getContentPane().add(noiDung, BorderLayout.CENTER);
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.pack();

        datNut(false);
        btnKetNoi.addActionListener(e -> ketNoi());
        btnNgat.addActionListener(e -> ngat("Đã ngắt kết nối"));
        btnMicStart.addActionListener(e -> batMic());
        btnMicStop.addActionListener(e -> {
            PhienAmThanh p = phien;
            if (p != null) viecNen.submit(p::tatMic);
            btnMicStart.setEnabled(true);
            btnMicStop.setEnabled(false);
            lblThietBi.setText("Thiết bị: đã đóng");
        });
        btnAudioStart.addActionListener(e -> batLoa(true));
        btnAudioStop.addActionListener(e -> batLoa(false));
        sldNghe.addChangeListener(e -> {
            lblNghe.setText("Âm lượng nghe: " + sldNghe.getValue() + "%");
            PhienAmThanh p = phien;
            if (p == null) return;
            p.amLuongNghe = sldNghe.getValue() / 100.0;            // nghe thấy ngay khi kéo
            if (!sldNghe.getValueIsAdjusting()) {                    // thả chuột: báo server một lần
                double v = p.amLuongNghe;
                viecNen.submit(() -> p.datAmLuongNghe(v));
            }
        });
        sldMic.addChangeListener(e -> {
            lblMic.setText("Mic Volume: " + sldMic.getValue() + "%");
            PhienAmThanh p = phien;
            if (p != null) p.amLuongMic = sldMic.getValue() / 100.0;
        });
        hen.start();
    }

    static JPanel hang(JButton... nut) {
        JPanel p = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 0));
        p.setOpaque(false);
        for (JButton b : nut) {
            b.setPreferredSize(new Dimension(120, 30));
            p.add(b);
        }
        return p;
    }

    void datNut(boolean daKetNoi) {
        btnKetNoi.setEnabled(!daKetNoi);
        btnNgat.setEnabled(daKetNoi);
        txtIp.setEnabled(!daKetNoi);
        txtCong.setEnabled(!daKetNoi);
        txtTen.setEnabled(!daKetNoi);
        btnMicStart.setEnabled(daKetNoi);
        btnMicStop.setEnabled(false);
        btnAudioStart.setEnabled(daKetNoi);
        btnAudioStop.setEnabled(false);
    }

    /** Nhật ký của phiên (luồng nền) -> dòng sự kiện trên cửa sổ (EDT). */
    void baoTuPhien(String s) {
        System.out.println("[" + txtTenAnToan + "] " + s);
        SwingUtilities.invokeLater(() -> {
            lblSuKien.setText(s);
            if (s.startsWith("Bị từ chối")) trangThaiRieng = s;
            else if (s.startsWith("Mic: mở")) trangThaiRieng = null;
        });
    }

    volatile String txtTenAnToan = "";                               // tên đọc được từ luồng nền

    void ketNoi() {
        String host = txtIp.getText().strip(), ten = txtTen.getText().strip();
        int cong;
        try {
            cong = Integer.parseInt(txtCong.getText().strip());
        } catch (NumberFormatException e) {
            lblTrangThai.setText("Cổng không hợp lệ");
            return;
        }
        if (ten.isEmpty()) {
            lblTrangThai.setText("Chưa nhập tên");
            return;
        }
        txtTenAnToan = ten;
        btnKetNoi.setEnabled(false);
        lblTrangThai.setText("Đang kết nối " + host + ":" + cong + "…");
        lblTrangThai.setForeground(Color.BLACK);
        double nghe = sldNghe.getValue() / 100.0, mic = sldMic.getValue() / 100.0;
        viecNen.submit(() -> {
            PhienAmThanh p = new PhienAmThanh(ten, this::baoTuPhien,
                    () -> SwingUtilities.invokeLater(() -> ngat("Mất kết nối tới server")));
            p.amLuongMic = mic;
            try {
                p.ketNoi(host, cong);
                if (nghe != 1) p.datAmLuongNghe(nghe);
                phien = p;
                SwingUtilities.invokeLater(() -> {
                    datNut(true);
                    trangThaiRieng = null;
                });
            } catch (IOException e) {
                p.ngat();
                SwingUtilities.invokeLater(() -> {
                    lblTrangThai.setText("Không kết nối được: " + e.getMessage());
                    lblTrangThai.setForeground(new Color(170, 0, 0));
                    btnKetNoi.setEnabled(true);
                });
            }
        });
    }

    void ngat(String lyDo) {
        PhienAmThanh p = phien;
        phien = null;
        if (p != null) viecNen.submit(p::ngat);
        datNut(false);
        lblTrangThai.setText(lyDo);
        lblTrangThai.setForeground(Color.BLACK);
        lblServerVol.setText("Server Vol: --");
        lblThietBi.setText("Thiết bị: chưa mở");
        lblLoa.setText("Loa: tắt (chỉ đo mức)");
    }

    /** Start Mic: mở thiết bị đã chọn (hoặc giả lập) trên luồng nền rồi xin server bật mic. */
    void batMic() {
        PhienAmThanh p = phien;
        if (p == null) return;
        String chon = (String) cboMic.getSelectedItem();
        btnMicStart.setEnabled(false);
        viecNen.submit(() -> {
            ThietBiAmThanh.NguonKhung n = ThietBiAmThanh.moNguon(chon, tanSo, this::baoTuPhien);
            p.batMic(n);
            SwingUtilities.invokeLater(() -> {
                lblThietBi.setText(n.that() ? "Đang dùng micro THẬT" : "Đang dùng GIẢ LẬP (" + KhachAmThanh.fmtG(tanSo) + " Hz)");
                lblThietBi.setForeground(n.that() ? new Color(0, 110, 0) : new Color(0, 70, 160));
                btnMicStop.setEnabled(true);
            });
        });
    }

    /** Start/Stop Audio: phát (hoặc thôi phát) âm thanh nhận được ra loa; không có loa thì chỉ đo mức. */
    void batLoa(boolean bat) {
        PhienAmThanh p = phien;
        if (p == null) return;
        btnAudioStart.setEnabled(!bat);
        btnAudioStop.setEnabled(bat);
        viecNen.submit(() -> {
            ThietBiAmThanh.LoaPhat l = bat ? ThietBiAmThanh.LoaPhat.mo() : null;
            p.datLoa(l);
            String chu = !bat ? "Loa: tắt (chỉ đo mức)" : l != null ? "Loa: THẬT – đang phát" : "Không có loa – chỉ đo mức";
            SwingUtilities.invokeLater(() -> lblLoa.setText(chu));
        });
    }

    /** Bộ hẹn giờ (EDT): đọc số liệu của phiên, cập nhật nhãn và thanh đo. */
    void capNhat() {
        PhienAmThanh p = phien;
        double mn = p != null ? p.mucNhan : Double.NEGATIVE_INFINITY, mm = p != null ? p.mucMic : Double.NEGATIVE_INFINITY;
        mucNhan.datMuc(mn);
        lblMucNhan.setText(ThanhMuc.chu(mn));
        mucMic.datMuc(mm);
        lblMucMic.setText(ThanhMuc.chu(mm));
        if (p == null) return;
        double g = p.doLoiServer;
        lblServerVol.setText(Double.isNaN(g) ? "Server Vol: --" : String.format(Locale.ROOT, "Server Vol: %.0f%%", g * 100));
        String mic = p.tuTat ? "tự tắt" : p.micMoServer ? "mở" : p.lyDoMic.contains("server") ? "bị khóa (server)" : "đang chờ server";
        if (trangThaiRieng != null && !p.micMoServer) {
            lblTrangThai.setText(trangThaiRieng);
            lblTrangThai.setForeground(new Color(170, 0, 0));
        } else {
            lblTrangThai.setText("Đã kết nối – mã " + p.ma + " – mic: " + mic);
            lblTrangThai.setForeground(new Color(0, 120, 0));
        }
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        String ten = args.length > 2 ? args[2] : "A";
        double tanSo = 440;
        for (int i = 3; i < args.length; i++) if (args[i].equals("--tan-so")) tanSo = Double.parseDouble(args[++i]);
        double f = tanSo;
        SwingUtilities.invokeLater(() -> {
            ServerAmThanhGui.caiDatGiaoDien();
            new KhachAmThanhGui(host, cong, ten, f).frame.setVisible(true);
        });
    }
}
