import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JComboBox;
import javax.swing.JFileChooser;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JSpinner;
import javax.swing.JTextField;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.filechooser.FileNameExtensionFilter;

/**
 * Bài mở rộng MR6.5 – Máy phát trực tiếp có giao diện Swing.
 *
 * Lõi MayPhat giữ kết nối, luồng âm thanh 20 ms và luồng video; cửa sổ chọn nguồn (camera thật qua ffmpeg / tệp video
 * / mẫu thử testsrc / giả lập clapper; micro thật qua javax.sound / tiếng của tệp video / giả lập), fps, GOP, bấm
 * Phát / Dừng phát / Khung khóa ngay; cứ 100 ms đọc trạng thái để vẽ xem trước, thanh mức micro, thống kê và danh
 * sách người xem. Thiết bị thật chỉ được mở khi bấm PHÁT.
 *
 * Chạy:  java -cp out PhatGui [tên] [host] [cổng_tcp]
 */
public class PhatGui {
    final JFrame frame;
    final JTextField txtIp, txtCong, txtTen, txtTep = new JTextField(30);
    final JButton btnKetNoi = GiaoDienChung.nut("Kết nối", GiaoDienChung.XANH_LA, 150);
    final List<String[]> dsCam = new ArrayList<>();                 // {mã, tên hiển thị}
    final List<String[]> dsMic = new ArrayList<>();                 // {mã (tên mixer, "tep" hoặc null), tên hiển thị}
    final JComboBox<String> cboCam = new JComboBox<>(), cboMic = new JComboBox<>();
    final JSpinner spnFps = new JSpinner(new SpinnerNumberModel(Chung.FPS_MAC_DINH, 1, 30, 1));
    final JSpinner spnGop = new JSpinner(new SpinnerNumberModel(Chung.GOP_MAC_DINH, 1, 300, 1));
    final JButton btnChonTep = GiaoDienChung.nut("Chọn tệp…", GiaoDienChung.XAM, 0),
            btnTaoMau = GiaoDienChung.nut("Tạo tệp mẫu", GiaoDienChung.XAM, 0);
    final JButton btnPhat = GiaoDienChung.nut("PHÁT", GiaoDienChung.XANH_LA, 100),
            btnDung = GiaoDienChung.nut("DỪNG PHÁT", GiaoDienChung.DO, 100),
            btnI = GiaoDienChung.nut("Khung khóa ngay", GiaoDienChung.XANH_DUONG, 140);
    final JLabel chiBao;
    final GiaoDienChung.KhungAnh khungAnh = new GiaoDienChung.KhungAnh(2 * Chung.RONG, 2 * Chung.CAO);
    final GiaoDienChung.ThanhMuc thanhMic = new GiaoDienChung.ThanhMuc(230);
    final Map<String, JLabel> nhanTk = new LinkedHashMap<>();
    final JLabel lblDs = GiaoDienChung.nhan("Người xem (0):");
    final DefaultListModel<String> mdlXem = new DefaultListModel<>();
    final GiaoDienChung.NhatKy nhatKy;
    final JLabel thanhTt = GiaoDienChung.thanhTrangThai("Trạng thái: chưa kết nối");
    final GiaoDienChung.ThongKeDinhKy dinhKy;
    String nhanTt = "chưa kết nối";
    volatile MayPhat lop;
    Long tsDaVe;
    final Timer hen = new Timer(100, e -> capNhat()), henGiay = new Timer(1000, e -> moiGiay());

    PhatGui(String ten, String host, int cong) {
        frame = new JFrame("Máy phát trực tiếp – MR6.5 (" + ten + ")");
        nhatKy = new GiaoDienChung.NhatKy(5, 80, "phat " + ten);
        dinhKy = new GiaoDienChung.ThongKeDinhKy("phat " + ten);
        txtIp = GiaoDienChung.oNhap(host, 12);
        txtCong = GiaoDienChung.oNhap(cong, 6);
        txtTen = GiaoDienChung.oNhap(ten, 9);
        boolean ff = Camera.coFfmpeg();
        dsCam.addAll(Camera.lietKeCamera());
        if (ff) {
            dsCam.add(new String[] {TepVideo.TEP, TepVideo.TEN_CAMERA});
            dsCam.add(new String[] {Camera.MAU_THU, Camera.TEN_MAU_THU});
        }
        dsCam.add(new String[] {Camera.GIA_LAP, Camera.TEN_GIA_LAP});
        for (String[] x : dsCam) cboCam.addItem(x[1]);
        cboCam.setSelectedIndex(dsCam.size() - 1);
        for (String m : ThietBiAmThanh.dsMicro()) dsMic.add(new String[] {m, m});
        if (ff) dsMic.add(new String[] {TepVideo.TEP, TepVideo.TEN_MIC});
        dsMic.add(new String[] {null, ThietBiAmThanh.TEN_GIA_LAP});
        for (String[] x : dsMic) cboMic.addItem(x[1]);
        cboMic.setSelectedIndex(dsMic.size() - 1);
        Dimension co = new Dimension(290, cboCam.getPreferredSize().height);
        cboCam.setPreferredSize(co);
        cboMic.setPreferredSize(co);

        JPanel tren = new JPanel();
        tren.setLayout(new BoxLayout(tren, BoxLayout.X_AXIS));
        tren.setBackground(GiaoDienChung.NEN);
        JPanel g = GiaoDienChung.khungNhom("Kết nối server");
        g.setLayout(new GridBagLayout());
        GridBagConstraints c = new GridBagConstraints();
        c.anchor = GridBagConstraints.WEST;
        c.insets = new Insets(1, 2, 1, 2);
        String[] nhan = {"Server IP:", "Port:", "Tên:"};
        JTextField[] o = {txtIp, txtCong, txtTen};
        for (int i = 0; i < 3; i++) {
            c.gridy = i; c.gridx = 0; g.add(GiaoDienChung.nhan(nhan[i]), c);
            c.gridx = 1; g.add(o[i], c);
        }
        c.gridy = 3; c.gridx = 0; c.gridwidth = 2; c.anchor = GridBagConstraints.CENTER; c.insets = new Insets(4, 2, 1, 2);
        g.add(btnKetNoi, c);
        g.setMaximumSize(new Dimension(g.getPreferredSize().width, Integer.MAX_VALUE));
        tren.add(g);

        g = GiaoDienChung.khungNhom("Nguồn và tham số");
        g.setLayout(new GridBagLayout());
        c = new GridBagConstraints();
        c.anchor = GridBagConstraints.WEST;
        c.insets = new Insets(1, 2, 1, 2);
        c.gridy = 0; c.gridx = 0; g.add(GiaoDienChung.nhan("Camera:"), c);
        c.gridx = 1; c.gridwidth = 4; g.add(cboCam, c);
        c.gridwidth = 1; c.gridy = 1; c.gridx = 0; g.add(GiaoDienChung.nhan("Micro:"), c);
        c.gridx = 1; c.gridwidth = 4; g.add(cboMic, c);
        c.gridwidth = 1; c.gridy = 2; c.gridx = 0; g.add(GiaoDienChung.nhan("FPS:"), c);
        c.gridx = 1; g.add(spnFps, c);
        c.gridx = 2; c.anchor = GridBagConstraints.EAST; g.add(GiaoDienChung.nhan("GOP (khung):"), c);
        c.gridx = 3; c.anchor = GridBagConstraints.WEST; g.add(spnGop, c);
        c.gridy = 3; c.gridx = 0; g.add(GiaoDienChung.nhan("Tệp video:"), c);
        JPanel f = GiaoDienChung.hang(txtTep, btnChonTep, btnTaoMau);
        ((FlowLayout) f.getLayout()).setHgap(0);
        btnChonTep.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createEmptyBorder(0, 4, 0, 0), btnChonTep.getBorder()));
        c.gridx = 1; c.gridwidth = 4; g.add(f, c);
        if (!ff) for (javax.swing.JComponent w : new javax.swing.JComponent[] {txtTep, btnChonTep, btnTaoMau}) w.setEnabled(false);
        f = GiaoDienChung.hang(btnPhat, btnDung, btnI);
        c.gridy = 4; c.gridx = 0; c.gridwidth = 5; c.insets = new Insets(4, 0, 1, 2); g.add(f, c);
        for (JButton b : new JButton[] {btnPhat, btnDung, btnI}) b.setEnabled(false);
        chiBao = GiaoDienChung.nhan("Camera: tắt | Micro: tắt" + (ff ? "" : " (không có ffmpeg)"));
        chiBao.setForeground(new Color(0x404040));
        c.gridy = 5; c.insets = new Insets(1, 2, 1, 2); g.add(chiBao, c);
        tren.add(g);

        JPanel giua = new JPanel();
        giua.setLayout(new BoxLayout(giua, BoxLayout.X_AXIS));
        giua.setBackground(GiaoDienChung.NEN);
        g = GiaoDienChung.khungNhom("Xem trước (khung vừa gửi)");
        g.setLayout(new BorderLayout(0, 4));
        g.add(khungAnh, BorderLayout.CENTER);
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Mức micro:"), thanhMic), BorderLayout.SOUTH);
        g.setMaximumSize(new Dimension(g.getPreferredSize().width, Integer.MAX_VALUE));
        giua.add(g);
        g = GiaoDienChung.khungNhom("Thống kê");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        String[][] muc = {{"ip", "Khung I / P đã gửi"}, {"tb", "Kích thước TB I / P"}, {"pmin", "P nhỏ nhất / lớn nhất"},
                {"kb", "kbit/s tiếng / hình"}, {"pli", "PLI (xin khung khóa) nhận"}};
        for (String[] m : muc) {
            g.add(GiaoDienChung.nhan(m[1] + ":"));
            JLabel l = GiaoDienChung.nhanDam("–");
            l.setBorder(BorderFactory.createEmptyBorder(0, 12, 0, 0));
            nhanTk.put(m[0], l);
            g.add(l);
        }
        lblDs.setBorder(BorderFactory.createEmptyBorder(4, 0, 0, 0));
        g.add(lblDs);
        JList<String> lst = new JList<>(mdlXem);
        lst.setVisibleRowCount(4);
        JScrollPane cuon = new JScrollPane(lst);
        cuon.setAlignmentX(0);
        cuon.setPreferredSize(new Dimension(230, 76));
        g.add(cuon);
        giua.add(g);

        g = GiaoDienChung.khungNhom("Nhật ký");
        g.setLayout(new BorderLayout());
        g.add(nhatKy, BorderLayout.CENTER);
        JPanel duoi = new JPanel(new BorderLayout(0, 4));
        duoi.setBackground(GiaoDienChung.NEN);
        duoi.add(g, BorderLayout.CENTER);
        duoi.add(thanhTt, BorderLayout.SOUTH);

        JPanel goc = new JPanel(new BorderLayout(0, 4));
        goc.setBackground(GiaoDienChung.NEN);
        goc.setBorder(BorderFactory.createEmptyBorder(4, 6, 6, 6));
        goc.add(tren, BorderLayout.NORTH);
        goc.add(giua, BorderLayout.CENTER);
        goc.add(duoi, BorderLayout.SOUTH);
        frame.setContentPane(goc);
        frame.setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        frame.addWindowListener(new java.awt.event.WindowAdapter() {
            @Override public void windowClosing(java.awt.event.WindowEvent e) { dong(); System.exit(0); }
        });
        frame.setResizable(false);
        frame.pack();

        btnKetNoi.addActionListener(e -> bamKetNoi());
        btnPhat.addActionListener(e -> batPhat());
        btnDung.addActionListener(e -> dungPhat());
        btnI.addActionListener(e -> { if (lop != null) lop.khungKhoaNgay("nút Khung khóa ngay"); });
        spnFps.addChangeListener(e -> apThamSo());
        spnGop.addChangeListener(e -> apThamSo());
        cboCam.addActionListener(e -> doiCamera());
        btnChonTep.addActionListener(e -> chonTep());
        btnTaoMau.addActionListener(e -> taoMau());
        hen.start();
        henGiay.start();
    }

    private void ghi(String text) { SwingUtilities.invokeLater(() -> nhatKy.ghi(text)); }

    // ------------------------------------------------------------ kết nối
    void bamKetNoi() {
        if (lop != null && lop.daKetNoi) {
            lop.dong();
            daNgat("đã ngắt kết nối");
            return;
        }
        String ten = txtTen.getText().strip(), host = txtIp.getText().strip();
        int cong;
        try {
            cong = Integer.parseInt(txtCong.getText().strip());
        } catch (NumberFormatException e) {
            nhanTt = "cổng không hợp lệ";
            return;
        }
        if (lop != null) lop.dong();
        MayPhat l = new MayPhat(ten, this::ghi, 0.0);
        lop = l;
        btnKetNoi.setEnabled(false);
        btnKetNoi.setText("Đang kết nối…");
        Chung.chayNen("ket-noi", () -> {                              // connect có thể chờ tới 5 s
            try {
                int ma = l.ketNoi(host, cong);
                SwingUtilities.invokeLater(() -> daKetNoi(ma));
            } catch (IOException e) {
                SwingUtilities.invokeLater(() -> daNgat("không kết nối được – " + e.getMessage()));
            }
        });
    }

    void daKetNoi(int ma) {
        btnKetNoi.setEnabled(true);
        btnKetNoi.setText("Ngắt kết nối");
        btnKetNoi.setBackground(GiaoDienChung.DO);
        nhanTt = "đã kết nối, mã " + ma;
        btnPhat.setEnabled(true);
        for (JTextField o : new JTextField[] {txtIp, txtCong, txtTen}) o.setEnabled(false);
    }

    void daNgat(String text) {
        btnKetNoi.setEnabled(true);
        btnKetNoi.setText("Kết nối");
        btnKetNoi.setBackground(GiaoDienChung.XANH_LA);
        nhanTt = text;
        for (JButton b : new JButton[] {btnPhat, btnDung, btnI}) b.setEnabled(false);
        cboCam.setEnabled(true);
        cboMic.setEnabled(true);
        for (JTextField o : new JTextField[] {txtIp, txtCong, txtTen}) o.setEnabled(true);
    }

    // ------------------------------------------------------------ phát
    void batPhat() {
        MayPhat l = lop;
        if (l == null || !l.daKetNoi) return;
        String[] cam0 = dsCam.get(cboCam.getSelectedIndex()), mic0 = dsMic.get(cboMic.getSelectedIndex());
        int fps = (Integer) spnFps.getValue(), gop = (Integer) spnGop.getValue();
        boolean micTep = TepVideo.TEP.equals(mic0[0]);
        Camera.NguonCamera cam = null;
        ThietBiAmThanh.NguonAm mic = null;
        if (cam0[0].equals(TepVideo.TEP)) {                              // tệp video: hình (và tiếng nếu chọn)
            try {
                TepVideo.MoTep m = TepVideo.moTep(txtTep.getText().strip(), fps, micTep);
                cam = m.camera();
                mic = m.mic();
                nhatKy.ghi("Phát tệp " + m.camera().tep.ten + (mic != null ? " (cả tiếng)" : ""));
            } catch (IOException e) {
                nhatKy.ghi("Không mở được tệp video (" + e.getMessage() + "); dùng camera giả lập");
                cam = null;
            }
        }
        if (cam == null)
            cam = Camera.mo(cam0[0].equals(TepVideo.TEP) ? Camera.GIA_LAP : cam0[0], cam0[1], l.ten, l.ma != null ? l.ma : 1, fps);
        if (mic == null) {
            if (micTep) nhatKy.ghi("Tiếng của tệp chỉ dùng được khi camera là tệp video; dùng micro giả lập");
            ThietBiAmThanh.KetQuaMic kq = ThietBiAmThanh.moMic(micTep ? null : mic0[0]);
            if (!kq.loi().isEmpty()) nhatKy.ghi(kq.loi());
            mic = kq.mic();
        }
        l.batDauPhat(cam, mic, fps, gop);
        nhatKy.ghi("Camera: " + cam.moTa());
        nhatKy.ghi("Micro: " + mic.moTa());
        chiBao.setText("Camera: " + cam.moTa() + " | Micro: " + mic.moTa());
        chiBao.setForeground(cam.that() || mic.that() ? new Color(0x006000) : new Color(0x8a4b00));
        btnPhat.setEnabled(false);
        btnDung.setEnabled(true);
        btnI.setEnabled(true);
        cboCam.setEnabled(false);
        cboMic.setEnabled(false);
    }

    /** Chọn camera "Tệp video" -> micro tự chuyển sang tiếng của tệp (đổi lại được); bỏ "Tệp video" mà micro còn là
     *  tiếng của tệp -> micro về thiết bị đầu tiên trong danh sách (micro thật nếu có). */
    void doiCamera() {
        int i = cboCam.getSelectedIndex();
        if (i >= 0 && dsCam.get(i)[0].equals(TepVideo.TEP)) {
            for (int k = 0; k < dsMic.size(); k++) if (TepVideo.TEP.equals(dsMic.get(k)[0])) cboMic.setSelectedIndex(k);
        } else if (cboMic.getSelectedIndex() >= 0 && TepVideo.TEP.equals(dsMic.get(cboMic.getSelectedIndex())[0])) {
            cboMic.setSelectedIndex(0);
        }
    }

    void chonTep() {
        JFileChooser fc = new JFileChooser(new File(".").getAbsoluteFile());
        fc.setDialogTitle("Chọn tệp video");
        fc.addChoosableFileFilter(new FileNameExtensionFilter("Tệp video", "mp4", "mkv", "avi", "mov", "webm", "flv", "ts", "m4v"));
        fc.setFileFilter(fc.getChoosableFileFilters()[1]);
        if (fc.showOpenDialog(frame) == JFileChooser.APPROVE_OPTION) datTep(fc.getSelectedFile().getPath());
    }

    void datTep(String duong) {
        txtTep.setText(duong);
        for (int i = 0; i < dsCam.size(); i++) if (dsCam.get(i)[0].equals(TepVideo.TEP)) cboCam.setSelectedIndex(i);
        doiCamera();
    }

    /** Tạo tệp mẫu 10 s (chớp trắng + bíp mỗi giây) trong thư mục out/ cạnh chương trình, rồi chọn nó. */
    void taoMau() {
        File tep = new File("out", "tep_mau.mp4").getAbsoluteFile();
        tep.getParentFile().mkdirs();
        btnTaoMau.setEnabled(false);
        btnTaoMau.setText("Đang tạo…");
        Chung.chayNen("tao-tep-mau", () -> {
            String loi = "";
            try {
                TepVideo.taoTepMau(tep.getPath(), 10);
            } catch (IOException e) {
                loi = e.getMessage();
            }
            String l = loi;
            SwingUtilities.invokeLater(() -> {
                btnTaoMau.setEnabled(true);
                btnTaoMau.setText("Tạo tệp mẫu");
                if (l.isEmpty()) {
                    nhatKy.ghi("Đã tạo tệp mẫu " + tep.getPath());
                    datTep(tep.getPath());
                } else {
                    nhatKy.ghi("Không tạo được tệp mẫu: " + l);
                }
            });
        });
    }

    void dungPhat() {
        if (lop != null) lop.dungPhat();
        chiBao.setText("Camera: tắt | Micro: tắt");
        chiBao.setForeground(new Color(0x404040));
        btnPhat.setEnabled(true);
        btnDung.setEnabled(false);
        btnI.setEnabled(false);
        cboCam.setEnabled(true);
        cboMic.setEnabled(true);
    }

    /** Giữ mũi tên của ô FPS/GOP đổi giá trị liên tục: chỉ áp dụng (và gửi thong_tin) khi giá trị đứng yên 400 ms. */
    final javax.swing.Timer henThamSo = new javax.swing.Timer(400, e -> apThamSoThat());
    { henThamSo.setRepeats(false); }

    void apThamSo() {
        henThamSo.restart();
    }

    void apThamSoThat() {
        if (lop != null && lop.dangPhat) lop.datThamSo((Integer) spnFps.getValue(), (Integer) spnGop.getValue());
    }

    // ------------------------------------------------------------ vẽ lại
    void moiGiay() {
        if (lop != null) lop.thongKeGiay();                            // cập nhật kbit/s của giây vừa qua
    }

    void capNhat() {
        MayPhat l = lop;
        if (l == null) return;
        MayPhat.TrangThai tt = l.trangThai();
        thanhMic.dat(tt.mucMic);
        if (tt.dangPhat)
            dinhKy.inNeuDenHan(() -> Chung.f("I %d P %d | hình %.0f tiếng %.0f kbit/s | mic %s dBFS | PLI %d | %d người xem",
                    tt.soI, tt.soP, tt.kbpsHinh, tt.kbpsAm, GiaoDienChung.db(tt.mucMic), tt.pliNhan, tt.nguoiXem.size()));
        MayPhat.XemTruoc x = tt.xemTruoc;
        if (x != null && tt.dangPhat && !Long.valueOf(x.ts()).equals(tsDaVe)) {
            khungAnh.datAnh(GiaoDienChung.anh(x.rgb(), x.w(), x.h()));
            tsDaVe = x.ts();
        } else if (!tt.dangPhat) {
            khungAnh.datAnh(null);
        }
        nhanTk.get("ip").setText(tt.soI + " / " + tt.soP);
        nhanTk.get("tb").setText(Chung.f("%.0f B / %.0f B", tt.tbI, tt.tbP));
        nhanTk.get("pmin").setText(tt.pMin + " B / " + tt.pMax + " B");
        nhanTk.get("kb").setText(Chung.f("%.0f / %.0f", tt.kbpsAm, tt.kbpsHinh));
        nhanTk.get("pli").setText(String.valueOf(tt.pliNhan));
        List<String> hien = new ArrayList<>();
        for (int i = 0; i < mdlXem.size(); i++) hien.add(mdlXem.get(i));
        if (!hien.equals(tt.nguoiXem)) {
            mdlXem.clear();
            for (String t : tt.nguoiXem) mdlXem.addElement(t);
        }
        lblDs.setText("Người xem (" + tt.nguoiXem.size() + "):");
        if (nhanTt.startsWith("đã kết nối") && !tt.daKetNoi) daNgat("mất kết nối server");
        String trang = tt.dangPhat ? "ĐANG PHÁT " + tt.fps + " fps, GOP " + tt.gop + " – " + tt.nguoiXem.size() + " người xem" : nhanTt;
        thanhTt.setText("Trạng thái: " + trang);
    }

    void dong() {
        hen.stop();
        henGiay.stop();
        if (lop != null) lop.dong();
        frame.dispose();
    }

    public static void main(String[] args) {
        String ten = args.length > 0 ? args[0] : "P";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43652;
        GiaoDienChung.NhatKy.inRa = true;                           // chạy riêng: nhật ký cũng ra dòng lệnh
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new PhatGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
