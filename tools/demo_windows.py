#!/usr/bin/env python3
"""Chạy kịch bản minh họa (demo.json) của các bài trên WINDOWS và lưu kết quả thực thi thật theo hệ điều hành.

Cùng kịch bản với tools/demo.py (Linux), nhưng:
  - Kết quả lưu với hậu tố _windows, không đè tệp của Linux:
        ket_qua_windows.txt   toàn bộ đầu ra các tiến trình
        ket_qua_windows.png   ảnh các cửa sổ dòng lệnh (kiểu Windows)
        ket_qua_gui_windows.png, ket_qua_gui_tep_windows.png, ...   ảnh do chính chương trình giao diện chụp
    (chương trình giao diện vẫn ghi ket_qua_gui.png...; công cụ đổi tên tệp vừa sinh thành *_windows.* rồi trả
    lại tệp Linux như cũ).
  - Lệnh Linux được chuyển sang Windows: python3 -> trình Python đang chạy công cụ này; bỏ xvfb-run (Windows có
    màn hình thật); out/app, ../../target/release/x -> .exe; env K=V lệnh; timeout -s INT N lệnh; rm; ls; java
    thêm -Dstdout.encoding=UTF-8. Lệnh sh/bash cần khai báo bản Windows trong demo.json (xem dưới).
  - Mỗi tiến trình chạy trong một console riêng (ẩn) đặt mã trang UTF-8 (65001); dừng tiến trình nền bằng Ctrl+C
    gửi riêng vào console đó (như người dùng bấm Ctrl+C), quá 3 s thì buộc dừng cả cây tiến trình.

Trường bổ sung (tùy chọn) trong demo.json cho Windows:
  "setup_windows": [[...], ...]               thay cho "setup"
  "bo_qua_windows": "lý do"                  bỏ qua cả bài trên Windows
  "titles_windows": {"tên": "tiêu đề"}       tiêu đề cửa sổ riêng cho ảnh Windows
  trong một tiến trình:
  "cmd_windows": [...] hoặc [[...], [...]]   thay cho "cmd"; danh sách lệnh thì chạy lần lượt (như ;)
  "dung_khi_loi_windows": true               với danh sách lệnh: dừng ở lệnh lỗi đầu tiên (như &&)
  "show_windows": [...] hoặc "chuỗi"         dòng lệnh in đầu nhật ký (mặc định suy ra từ cmd)
  "bo_qua_windows": "lý do"                  bỏ qua tiến trình này
Lệnh dựng sẵn dùng trong cmd_windows (không cần công cụ ngoài):
  ["@ctrl_c", "<tên tiến trình nền>"]        gửi Ctrl+C tới tiến trình nền cùng kịch bản
  ["@echo", "chuỗi"...]   ["@ls", "thư mục"]   ["@rm", "tệp"...]
  ["@sha256_16", "tệp"...]                   16 ký tự đầu SHA-256 + tên tệp (như sha256sum | cut -c1-16,65-)
  ["@head_c", "N", "nguồn", "đích"]          chép N byte đầu
  ["@dong_cuoi", "N", lệnh...]               chạy lệnh, chỉ giữ N dòng cuối đầu ra (như | tail -n N)

Cách dùng (từ thư mục gốc kho mã, trong Command Prompt hoặc PowerShell):
  py -3 tools\\demo_windows.py                          chạy tất cả bài (lâu: vài giờ)
  py -3 tools\\demo_windows.py python java\\Chuong03     theo ngôn ngữ / chương / thư mục bài
  py -3 tools\\demo_windows.py MR6.6 Bai3.1             theo mã bài (mọi ngôn ngữ)
  Tùy chọn:  --ngon-ngu csharp,go   --bo-qua-da-chay   --khong-gui   --chi-gui   --tu <mục>   --liet-ke
Cần: Python 3.10+ có Pillow (py -3 -m pip install --user pillow) và công cụ của ngôn ngữ cần chạy
(xem docs/CaiDatMoiTruong.md). Khi chạy các bài có giao diện, công cụ mở cửa sổ thật và chụp ảnh:
không dùng chuột, bàn phím trong lúc chạy.
"""
import argparse
import glob
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
GOC = os.path.dirname(HERE)
LA_WINDOWS = os.name == 'nt'
NGON_NGU = ['csharp', 'java', 'python', 'rust', 'go']
DUOI = '_windows'
LOG_DIR = os.path.join(GOC, '_log', 'demo_windows')

# Gợi ý cài đặt khi thiếu công cụ (winget có sẵn trên Windows 10/11) – đồng bộ với docs/CaiDatMoiTruong.md
GOI_Y = {
    'dotnet': 'winget install Microsoft.DotNet.SDK.10',
    'java': 'winget install -e --id EclipseAdoptium.Temurin.21.JDK',
    'javac': 'winget install -e --id EclipseAdoptium.Temurin.21.JDK',
    'go': 'winget install -e --id GoLang.Go',
    'cargo': 'winget install -e --id Rustlang.Rustup  (và Visual Studio Build Tools – xem docs/CaiDatMoiTruong.md)',
    'gcc': 'winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT',
    'ffmpeg': 'winget install -e --id Gyan.FFmpeg',
    'mosquitto': 'winget install -e --id EclipseFoundation.Mosquitto',
    'openssl': 'winget install -e --id ShiningLight.OpenSSL.Light',
    'protoc': 'winget install -e --id Google.Protobuf',
    'mvn': 'cần JDK; công cụ tự tải Apache Maven khi đã có Java',
    'curl': 'curl.exe có sẵn từ Windows 10 1803',
}
# Thư mục cài mặc định của một số công cụ không tự thêm vào PATH
CHO_CAI = {
    'mosquitto': [r'%ProgramFiles%\mosquitto'],
    'openssl': [r'%ProgramFiles%\OpenSSL-Win64\bin', r'%ProgramFiles%\OpenSSL\bin'],
    'go': [r'%ProgramFiles%\Go\bin'],
    'cargo': [r'%USERPROFILE%\.cargo\bin'],
}

# ------------------------------------------------------------------------------------------------ tiện ích Windows
CREATE_NO_WINDOW = 0x08000000
DETACHED_PROCESS = 0x00000008

# Trình khởi chạy: chạy trong console mới (ẩn) của từng tiến trình, đặt mã trang UTF-8 cho console đó (để .NET,
# lệnh hệ thống in tiếng Việt đúng), bỏ qua Ctrl+C của chính nó và chờ chương trình con (cùng console, cùng
# stdin/stdout đã chuyển hướng) kết thúc.
KHOI_CHAY = r'''
import ctypes, json, signal, subprocess, sys
k = ctypes.windll.kernel32
k.SetConsoleCP(65001); k.SetConsoleOutputCP(65001)
signal.signal(signal.SIGINT, lambda *a: None)
signal.signal(signal.SIGBREAK, lambda *a: None)
a = json.loads(sys.argv[1])
p = subprocess.Popen(a["cmd"], cwd=a["cwd"], stdin=sys.stdin.fileno(), stdout=sys.stdout.fileno(),
                     stderr=sys.stderr.fileno())
while True:
    try:
        sys.exit(p.wait())
    except KeyboardInterrupt:
        pass
'''
# Gửi Ctrl+C vào console của một tiến trình (tiến trình gửi không có console riêng, gắn tạm vào console đích)
GUI_CTRL_C = ('import ctypes,sys;k=ctypes.windll.kernel32;k.FreeConsole();ok=k.AttachConsole(int(sys.argv[1]));'
              'k.SetConsoleCtrlHandler(None,True);k.GenerateConsoleCtrlEvent(0,0);sys.exit(0 if ok else 1)')


def ctrl_c(proc):
    if proc.poll() is not None:
        return
    if LA_WINDOWS:
        subprocess.run([sys.executable, '-c', GUI_CTRL_C, str(proc.pid)], creationflags=DETACHED_PROCESS,
                       timeout=10, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        import signal
        os.killpg(proc.pid, signal.SIGINT)


def dung_cay(proc):
    if proc.poll() is None:
        if LA_WINDOWS:
            subprocess.run(['taskkill', '/T', '/F', '/PID', str(proc.pid)], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
        else:
            proc.kill()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass


def dung_nhe(proc, cho=3.0):
    """Ctrl+C rồi chờ; quá hạn thì buộc dừng cả cây tiến trình."""
    try:
        ctrl_c(proc)
        proc.wait(timeout=cho)
    except Exception:
        pass
    dung_cay(proc)


# ------------------------------------------------------------------------------------------------ chuyển lệnh
EXE_RE = re.compile(r'(^|[\\/])(out|target[\\/](release|debug))[\\/][A-Za-z0-9_.\-]+$')


def them_exe(a):
    """out/app, ../../target/release/x -> có đuôi .exe (tệp chạy trên Windows)."""
    if EXE_RE.search(a) and not a.lower().endswith('.exe') and '.' not in os.path.basename(a):
        return a + '.exe'
    return a


def la_python(c):
    return c in ('python3', 'python') or re.fullmatch(r'(/usr(/local)?/bin/)?python3(\.\d+)?', c) is not None


class Lenh:
    """Một lệnh đã chuyển sang Windows: argv, biến môi trường thêm, hạn chạy và cách dừng."""

    def __init__(self, argv, env=None, han=None, dung_ctrl_c=False, dung_san=None, giu_cuoi=None):
        self.argv, self.env = argv, env or {}
        self.han = han                    # giây (timeout -s INT N)
        self.dung_ctrl_c = dung_ctrl_c    # hết hạn thì Ctrl+C (không in "hết thời gian")
        self.dung_san = dung_san          # lệnh dựng sẵn (@...)
        self.giu_cuoi = giu_cuoi          # chỉ giữ N dòng cuối


def chuyen(cmd):
    """Chuyển một lệnh demo.json (dạng Linux) sang Windows. Trả về Lenh hoặc nêu lỗi ValueError."""
    cmd = list(cmd)
    env, han, ctrl, giu = {}, None, False, None
    while True:
        if not cmd:
            raise ValueError('lệnh rỗng')
        c = cmd[0]
        if c == 'xvfb-run':                                  # Windows có màn hình thật
            i = 1
            while i < len(cmd) and cmd[i].startswith('-'):
                i += 2 if cmd[i] in ('-s', '-n', '-e', '-f', '-p', '--server-args', '--server-num') else 1
            cmd = cmd[i:]
        elif c == 'env':
            i = 1
            while i < len(cmd) and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*=.*', cmd[i]):
                k, v = cmd[i].split('=', 1); env[k] = v; i += 1
            cmd = cmd[i:]
        elif c == 'timeout':
            i = 1
            while i < len(cmd) and cmd[i].startswith('-'):
                if cmd[i] in ('-s', '--signal', '-k', '--kill-after'):
                    i += 2
                else:
                    i += 1
            han = float(cmd[i].rstrip('s')); ctrl = True; cmd = cmd[i + 1:]
        elif c == '@dong_cuoi':
            giu = int(cmd[1]); cmd = cmd[2:]
        else:
            break
    c = cmd[0]
    if c.startswith('@') or c in ('rm', 'ls'):
        ten = {'rm': '@rm', 'ls': '@ls'}.get(c, c)
        return Lenh([ten] + [x for x in cmd[1:] if not (c in ('rm', 'ls') and x.startswith('-'))], env,
                    dung_san=ten)
    if c in ('sh', 'bash'):
        raise ValueError(f'lệnh {c} chưa có bản Windows – khai báo "cmd_windows" trong demo.json')
    argv = [sys.executable if la_python(c) else c] + cmd[1:]
    argv = [argv[0]] + [them_exe(a) for a in argv[1:]]
    argv[0] = them_exe(argv[0])
    if os.path.basename(argv[0]) in ('java', 'java.exe'):    # Java ghi ra tệp theo bảng mã của hệ thống
        argv[1:1] = ['-Dstdout.encoding=UTF-8', '-Dstderr.encoding=UTF-8']
        for i, a in enumerate(argv[:-1]):
            if a in ('-cp', '-classpath', '--class-path') and ':' in argv[i + 1]:
                argv[i + 1] = argv[i + 1].replace(':', ';')
    return Lenh(argv, env, han, ctrl, giu_cuoi=giu)


def chuyen_tien_trinh(p):
    """Danh sách Lenh của một tiến trình (cmd_windows có thể là chuỗi lệnh)."""
    goc = p.get('cmd_windows', p['cmd'])
    ds = goc if goc and isinstance(goc[0], list) else [goc]
    return [chuyen(x) for x in ds]


def hien_thi(p, ds):
    """Dòng lệnh in đầu nhật ký, dạng gõ trong Command Prompt."""
    if isinstance(p.get('show_windows'), str):
        return p['show_windows']
    if 'show_windows' in p:
        cac = [p['show_windows']] if not isinstance(p['show_windows'][0], list) else p['show_windows']
        return ' && '.join(' '.join(q(a) for a in x) for x in cac)
    if 'cmd_windows' not in p and 'show' in p:
        try:
            ds = [chuyen(p['show'])]
        except ValueError:
            pass
    phan = []
    for l in ds:
        a = list(l.argv)
        if l.dung_san:
            a[0] = a[0][1:]
        elif a[0] == sys.executable:
            a[0] = 'python'
        elif os.path.basename(a[0]).lower() in ('java', 'java.exe'):
            a = [a[0]] + [x for x in a[1:] if not x.startswith('-Dstd')]
        if '/' in a[0] and not a[0].startswith('@'):
            a[0] = a[0].replace('/', '\\')
        tien = ' '.join(f'set {k}={v} &&' for k, v in l.env.items())
        phan.append((tien + ' ' if tien else '') + ' '.join(q(x) for x in a))
    return ' && '.join(phan)


def q(a):
    return '"' + a.replace('"', '\\"') + '"' if (not a or re.search(r'[\s"&|<>^]', a)) else a


def tim_lenh(c, cwd):
    """Đường dẫn đầy đủ của tệp chạy: theo thư mục bài nếu là đường dẫn, ngược lại tìm trong PATH."""
    if os.sep in c or '/' in c or c.startswith('.'):
        p = os.path.normpath(os.path.join(cwd, c))
        return p if os.path.exists(p) else None
    w = shutil.which(c)
    if w:
        return w
    for d in CHO_CAI.get(c, []):
        d = os.path.expandvars(d)
        for e in ('.exe', '.cmd', '.bat', ''):
            if os.path.isfile(os.path.join(d, c + e)):
                return os.path.join(d, c + e)
    return None


# ------------------------------------------------------------------------------------------------ chạy lệnh
class ChayBai:
    def __init__(self, thu_muc, env):
        self.thu_muc, self.env = thu_muc, env
        self.nen = {}                  # tên -> Popen

    def mo(self, lenh, log, stdin_text, cwd, dong_stdin=True):
        """Khởi động một Lenh (không chờ). Trả về Popen. Tiến trình nền giữ stdin mở (như tools/demo.py)."""
        env = dict(self.env, **lenh.env)
        exe = tim_lenh(lenh.argv[0], cwd)
        if exe is None:
            raise FileNotFoundError(lenh.argv[0])
        argv = [exe] + lenh.argv[1:]
        stdin = subprocess.PIPE if stdin_text is not None else subprocess.DEVNULL
        if LA_WINDOWS:
            proc = subprocess.Popen([sys.executable, '-c', KHOI_CHAY, json.dumps({'cmd': argv, 'cwd': cwd})],
                                    cwd=cwd, env=env, stdin=stdin, stdout=log, stderr=subprocess.STDOUT,
                                    creationflags=CREATE_NO_WINDOW)
        else:
            proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=stdin, stdout=log, stderr=subprocess.STDOUT,
                                    start_new_session=True)
        if stdin_text is not None:
            try:
                proc.stdin.write(stdin_text.encode('utf-8')); proc.stdin.flush()
                if dong_stdin:
                    proc.stdin.close()
            except OSError:
                pass
        return proc

    def dung_san(self, lenh, f, cwd):
        """Lệnh dựng sẵn (@...)."""
        t, a = lenh.dung_san, lenh.argv[1:]
        ghi = lambda s: (f.write(s.encode('utf-8')), f.flush())
        if t == '@echo':
            ghi(' '.join(a) + '\n')
        elif t == '@ls':
            d = os.path.join(cwd, a[0] if a else '.')
            ghi(''.join(n + '\n' for n in sorted(os.listdir(d))))
        elif t == '@rm':
            for x in a:
                p = os.path.join(cwd, x)
                if os.path.isdir(p):
                    shutil.rmtree(p, ignore_errors=True)
                elif os.path.exists(p):
                    os.remove(p)
        elif t == '@sha256_16':
            for x in a:
                h = hashlib.sha256(open(os.path.join(cwd, x), 'rb').read()).hexdigest()
                ghi(f'{h[:16]}  {x}\n')
        elif t == '@head_c':
            n, src, dst = int(a[0]), a[1], a[2]
            with open(os.path.join(cwd, src), 'rb') as s, open(os.path.join(cwd, dst), 'wb') as d:
                d.write(s.read(n))
        elif t == '@ctrl_c':
            p = self.nen.get(a[0])
            if p is None:
                raise ValueError(f'không có tiến trình nền "{a[0]}"')
            ghi(f'(gửi Ctrl+C tới tiến trình "{a[0]}", PID {p.pid})\n')
            ctrl_c(p)
        else:
            raise ValueError(f'không biết lệnh {t}')
        return 0

    def chay_cho(self, lenh, f, stdin_text, cwd, han):
        """Chạy tiền cảnh và chờ. Trả về mã thoát (None nếu hết thời gian)."""
        if lenh.dung_san:
            return self.dung_san(lenh, f, cwd)
        if lenh.giu_cuoi:
            tam = f.name + '.tam'
            with open(tam, 'wb') as g:
                ma = self.chay_cho(Lenh(lenh.argv, lenh.env, lenh.han, lenh.dung_ctrl_c), g, stdin_text, cwd, han)
            dong = open(tam, 'rb').read().decode('utf-8', 'replace').splitlines()
            f.write(('\n'.join(dong[-lenh.giu_cuoi:]) + '\n').encode('utf-8')); f.flush()
            os.remove(tam)
            return ma
        proc = self.mo(lenh, f, stdin_text, cwd)
        han_that = lenh.han if lenh.han is not None else han
        try:
            return proc.wait(timeout=han_that)
        except subprocess.TimeoutExpired:
            if lenh.dung_ctrl_c:                     # timeout -s INT N: dừng như bấm Ctrl+C sau N giây
                dung_nhe(proc)
                return proc.returncode
            dung_nhe(proc, 1.0)
            f.write('[demo] Hết thời gian chờ, dừng tiến trình\n'.encode('utf-8')); f.flush()
            return None


# ------------------------------------------------------------------------------------------------ Maven, gRPC
def bao_dam_maven():
    """Maven không có gói winget: nếu đã có Java mà thiếu mvn, tải Apache Maven từ trang lưu trữ chính thức
    của Apache, kiểm tra SHA-512 rồi giải nén vào %LOCALAPPDATA%\\NetworkProgramming (dùng cho phiên chạy này)."""
    if shutil.which('mvn') or not shutil.which('java'):
        return
    ver = '3.9.9'
    base = os.path.join(os.environ.get('LOCALAPPDATA', os.path.expanduser('~')), 'NetworkProgramming')
    home = os.path.join(base, f'apache-maven-{ver}')
    if not os.path.isfile(os.path.join(home, 'bin', 'mvn.cmd')):
        os.makedirs(base, exist_ok=True)
        url = f'https://archive.apache.org/dist/maven/maven-3/{ver}/binaries/apache-maven-{ver}-bin.zip'
        print(f'  Tải Maven {ver} từ {url}')
        zp = os.path.join(base, f'apache-maven-{ver}-bin.zip')
        urllib.request.urlretrieve(url, zp)
        mong = urllib.request.urlopen(url + '.sha512').read().decode().split()[0].strip().lower()
        if hashlib.sha512(open(zp, 'rb').read()).hexdigest() != mong:
            os.remove(zp); print('  SHA-512 của Maven không khớp – bỏ qua'); return
        zipfile.ZipFile(zp).extractall(base); os.remove(zp)
    os.environ['PATH'] = os.path.join(home, 'bin') + os.pathsep + os.environ['PATH']
    print(f'  Dùng Maven tại {home}')


def bao_dam_protoc_go():
    if not shutil.which('go'):
        return
    gobin = os.path.join(subprocess.run(['go', 'env', 'GOPATH'], capture_output=True, text=True).stdout.strip(), 'bin')
    os.environ['PATH'] = gobin + os.pathsep + os.environ['PATH']
    for ten, goi in (('protoc-gen-go', 'google.golang.org/protobuf/cmd/protoc-gen-go@v1.35.2'),
                     ('protoc-gen-go-grpc', 'google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1')):
        if not shutil.which(ten):
            subprocess.run(['go', 'install', goi])


# ------------------------------------------------------------------------------------------------ tệp kết quả
def ten_windows(ten):
    goc, duoi = os.path.splitext(ten)
    return goc + DUOI + duoi


def chup_ket_qua(cac_thu_muc):
    """Ảnh chụp (nội dung, thời điểm) các tệp ket_qua* của Linux trước khi chạy."""
    kq = {}
    for d in cac_thu_muc:
        for p in glob.glob(os.path.join(d, 'ket_qua*')):
            if os.path.isfile(p) and not os.path.splitext(p)[0].endswith(DUOI):
                kq[p] = open(p, 'rb').read()
    return kq


def doi_ten_ket_qua(truoc, cac_thu_muc, bat_dau):
    """Tệp ket_qua* vừa được chương trình ghi -> *_windows.*; trả lại tệp Linux như trước khi chạy."""
    moi = []
    for d in cac_thu_muc:
        for p in glob.glob(os.path.join(d, 'ket_qua*')):
            if not os.path.isfile(p) or os.path.splitext(p)[0].endswith(DUOI):
                continue
            if p in truoc and truoc[p] == open(p, 'rb').read() and os.path.getmtime(p) < bat_dau - 1:
                continue                                   # tệp Linux không bị chạm tới
            dich = os.path.join(d, ten_windows(os.path.basename(p)))
            if p in truoc:
                shutil.copyfile(p, dich)
                open(p, 'wb').write(truoc[p])
            else:
                os.replace(p, dich)
            moi.append(os.path.relpath(dich, GOC))
    return moi


# ------------------------------------------------------------------------------------------------ một bài
def la_gui(p):
    c = ' '.join(p['cmd'])
    return 'xvfb-run' in c or 'GiaoDien' in c or '-windows' in c or p.get('name') == 'gui'


def chay_bai(thu_muc, tuy_chon):
    rel = os.path.relpath(thu_muc, GOC)
    spec = json.load(open(os.path.join(thu_muc, 'demo.json'), encoding='utf-8'))
    if spec.get('bo_qua_windows'):
        return 'BỎ QUA', spec['bo_qua_windows'], []
    if tuy_chon.khong_gui and any(la_gui(p) for p in spec['processes']) and \
            all(la_gui(p) for p in spec['processes']):
        return 'BỎ QUA', 'chỉ có giao diện (--khong-gui)', []
    if tuy_chon.chi_gui and not any(la_gui(p) for p in spec['processes']):
        return 'BỎ QUA', 'không có giao diện (--chi-gui)', []
    try:
        setup = [chuyen(c) for c in spec.get('setup_windows', spec.get('setup', []))]
        ds = []
        for p in spec['processes']:
            if p.get('bo_qua_windows') or (tuy_chon.khong_gui and la_gui(p)):
                continue
            ds.append((p, chuyen_tien_trinh(p)))
    except ValueError as e:
        return 'LỖI', str(e), []
    lenh_dau = {c[0] for c in spec.get('setup', [])} | {p['cmd'][0] for p in spec['processes']}
    if 'mvn' in lenh_dau and not tuy_chon.liet_ke:
        bao_dam_maven()
    if 'go-grpc_out' in json.dumps(spec) and not tuy_chon.liet_ke:
        bao_dam_protoc_go()
    cwd = thu_muc
    if spec.get('ascii_path') and re.search(r'[^\x00-\x7F]', thu_muc):
        # protoc (qua protobuf-maven-plugin) không đọc được đường dẫn có ký tự Unicode: chạy ở bản chép ASCII
        cwd = os.path.join(os.environ.get('TEMP', '/tmp'), 'NetworkProgramming', rel.replace(os.sep, '_'))
        shutil.rmtree(cwd, ignore_errors=True)
        shutil.copytree(thu_muc, cwd, ignore=shutil.ignore_patterns('target', 'out', 'bin', 'obj', '.log_*'))
        print(f'  Đường dẫn có ký tự Unicode – chạy tại bản chép {cwd}')
    thieu = sorted({l.argv[0] for l in setup + [l for _, ls in ds for l in ls]
                    if not l.dung_san and tim_lenh(l.argv[0], cwd) is None
                    and not ('/' in l.argv[0] or '\\' in l.argv[0] or l.argv[0].startswith('.'))})
    if thieu:
        return 'BỎ QUA', 'thiếu ' + ', '.join(f'{t} (cài: {GOI_Y.get(t, "xem docs/CaiDatMoiTruong.md")})'
                                               for t in thieu), []
    if tuy_chon.liet_ke:
        for l in setup:
            print('   [setup]', ' '.join(l.argv))
        for p, ls in ds:
            print(f'   [{p["name"]}]', hien_thi(p, ls), '(nền)' if p.get('background') else '')
        return 'LIỆT KÊ', '', []

    env = dict(os.environ, PYTHONUNBUFFERED='1', PYTHONIOENCODING='utf-8', PYTHONUTF8='1',
               DOTNET_CLI_TELEMETRY_OPTOUT='1', DOTNET_NOLOGO='1',
               MAVEN_OPTS=(os.environ.get('MAVEN_OPTS', '') + ' -Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8').strip())
    env.pop('JAVA_TOOL_OPTIONS', None)
    env.update(spec.get('env', {}))
    cac_thu_muc = [thu_muc] + ([os.path.dirname(thu_muc)] if os.path.basename(thu_muc) == 'GiaoDien' else [])
    truoc = chup_ket_qua(cac_thu_muc)
    bat_dau = time.time()
    chay = ChayBai(thu_muc, env)
    # bước chuẩn bị
    log_setup = os.path.join(thu_muc, '.log_setup.txt')
    with open(log_setup, 'wb') as f:
        for l in setup:
            ma = chay.chay_cho(l, f, None, cwd, 900)
            if ma != 0:
                f.close()
                return 'LỖI', f'bước chuẩn bị {" ".join(l.argv)} (mã {ma}) – xem {os.path.relpath(log_setup, GOC)}', []
    os.remove(log_setup)
    logs, mo = {}, []
    try:
        for p, ls in ds:
            ten = p['name']
            duong = os.path.join(thu_muc, f'.log_{ten}.txt')
            f = open(duong, 'wb')
            f.write(('> ' + hien_thi(p, ls) + '\n').encode('utf-8')); f.flush()
            logs[ten] = duong
            if p.get('background'):
                if len(ls) != 1 or ls[0].dung_san:
                    raise ValueError(f'tiến trình nền {ten} phải là một lệnh')
                proc = chay.mo(ls[0], f, p.get('stdin'), cwd, dong_stdin=False)
                chay.nen[ten] = proc
                mo.append((proc, f))
                time.sleep(p.get('wait', 1.0))
            else:
                for i, l in enumerate(ls):
                    ma = chay.chay_cho(l, f, p.get('stdin') if i == 0 else None, cwd, p.get('timeout', 30))
                    if ma != 0 and p.get('dung_khi_loi_windows'):
                        break
                f.close()
                time.sleep(p.get('wait', 0.3))
    except (FileNotFoundError, ValueError, OSError) as e:
        for proc, f in mo:
            dung_cay(proc); f.close()
        return 'LỖI', str(e), []
    for proc, f in mo:                                  # dừng như khi nhấn Ctrl+C
        dung_nhe(proc)
        f.close()
    if cwd != thu_muc:                                  # bản chép ASCII: lấy ảnh kết quả về thư mục bài
        for p in glob.glob(os.path.join(cwd, 'ket_qua*')):
            shutil.copy(p, thu_muc)
    ra_txt = os.path.join(thu_muc, 'ket_qua' + DUOI + '.txt')
    with open(ra_txt, 'w', encoding='utf-8', newline='\n') as out:
        for ten, duong in logs.items():
            out.write(f'===== {ten} =====\n' + open(duong, 'rb').read().decode('utf-8', 'replace')
                      .replace('\r\n', '\n') + '\n')
    # chuẩn hóa xuống dòng của nhật ký trước khi vẽ ảnh
    for duong in logs.values():
        s = open(duong, 'rb').read().decode('utf-8', 'replace').replace('\r\n', '\n')
        open(duong, 'w', encoding='utf-8', newline='\n').write(s)
    titles = dict(spec.get('titles', {}), **spec.get('titles_windows', {}))
    ra_png = os.path.join(thu_muc, 'ket_qua' + DUOI + '.png')
    r = subprocess.run([sys.executable, os.path.join(HERE, 'render_output.py'), '--kieu', 'windows', '-o', ra_png]
                       + [f"{titles.get(n, n)}:{logs[n]}" for n in logs], capture_output=True, text=True)
    for duong in logs.values():
        os.remove(duong)
    if r.returncode != 0:
        return 'LỖI', 'vẽ ảnh: ' + (r.stderr.strip().splitlines() or ['?'])[-1], []
    moi = [os.path.relpath(ra_txt, GOC), os.path.relpath(ra_png, GOC)] + doi_ten_ket_qua(truoc, cac_thu_muc, bat_dau)
    moi = list(dict.fromkeys(moi))
    txt = open(ra_txt, encoding='utf-8').read()
    canh_bao = []
    for mau in ('Traceback (most recent call last)', 'Unhandled exception', 'Exception in thread', 'panicked at',
                'Address already in use', 'Hết thời gian chờ', 'Kịch bản chờ quá lâu'):
        if mau in txt:
            canh_bao.append(mau)
    return ('ĐÃ CHẠY' if not canh_bao else 'CẦN XEM'), ', '.join(canh_bao), moi


# ------------------------------------------------------------------------------------------------ chọn bài
def tat_ca_bai():
    ds = []
    for lg in NGON_NGU:
        for p in glob.glob(os.path.join(GOC, lg, 'Chuong*', '*', 'demo.json')) + \
                 glob.glob(os.path.join(GOC, lg, 'Chuong*', '*', 'GiaoDien', 'demo.json')):
            ds.append(os.path.dirname(p))

    def khoa(d):
        rel = os.path.relpath(d, GOC).replace('\\', '/').split('/')
        so = [int(x) if x.isdigit() else x for x in re.split(r'(\d+)', rel[2])]
        return (NGON_NGU.index(rel[0]), rel[1], 'MR' in rel[2], so, len(rel))
    return sorted(ds, key=khoa)


def khop(d, mau):
    rel = os.path.relpath(d, GOC).replace('\\', '/')
    m = mau.replace('\\', '/').strip('/')
    if m in NGON_NGU:
        return rel.startswith(m + '/')
    if '/' in m:
        return rel == m or rel.startswith(m + '/')
    ten = rel.split('/')[2]                     # Bai3.1_EchoTCP, BaiMR6.6_DauGia
    return ten.split('_')[0] in ('Bai' + m, m, 'Bai' + m.removeprefix('Bai'))


def da_chay(d):
    kq = os.path.join(d, 'ket_qua' + DUOI + '.txt')
    if not os.path.exists(kq):
        return False
    t = os.path.getmtime(kq)
    for p in glob.glob(os.path.join(d, '**', '*'), recursive=True):
        if os.path.isfile(p) and not os.path.basename(p).startswith('ket_qua') and \
                not re.search(r'[\\/](out|bin|obj|target|__pycache__)[\\/]', p) and os.path.getmtime(p) > t:
            return False
    return True


def main():
    ap = argparse.ArgumentParser(description='Chạy demo.json trên Windows, lưu kết quả *_windows.*')
    ap.add_argument('muc', nargs='*', help='ngôn ngữ, thư mục (vd java\\Chuong03) hoặc mã bài (vd MR6.6, Bai3.1)')
    ap.add_argument('--ngon-ngu', help='chỉ các ngôn ngữ này, cách nhau dấu phẩy')
    ap.add_argument('--bo-qua-da-chay', action='store_true', help='bỏ bài đã có ket_qua_windows.txt mới hơn mã nguồn')
    ap.add_argument('--khong-gui', action='store_true', help='bỏ các tiến trình giao diện')
    ap.add_argument('--chi-gui', action='store_true', help='chỉ các bài có giao diện')
    ap.add_argument('--tu', help='bắt đầu từ bài này (tiếp tục lần chạy trước)')
    ap.add_argument('--liet-ke', action='store_true', help='chỉ in lệnh Windows sẽ chạy, không chạy')
    a = ap.parse_args()
    if not LA_WINDOWS and not a.liet_ke and not os.environ.get('DEMO_WINDOWS_THU_LINUX'):   # biến thử nội bộ
        sys.exit('Công cụ này dành cho Windows (trên Linux dùng tools/demo.py). Thử --liet-ke để xem lệnh.')
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    ds = tat_ca_bai()
    if a.muc:
        ds = [d for d in ds if any(khop(d, m) for m in a.muc)]
    if a.ngon_ngu:
        cho = a.ngon_ngu.split(',')
        ds = [d for d in ds if os.path.relpath(d, GOC).replace('\\', '/').split('/')[0] in cho]
    if a.tu:
        vt = next((i for i, d in enumerate(ds) if khop(d, a.tu)), None)
        if vt is None:
            sys.exit(f'Không thấy bài {a.tu}')
        ds = ds[vt:]
    if not ds:
        sys.exit('Không có bài nào khớp.')
    if not a.liet_ke:
        try:
            import PIL  # noqa: F401
        except ImportError:
            sys.exit('Thiếu Pillow: py -3 -m pip install --user pillow')
        os.makedirs(LOG_DIR, exist_ok=True)
    tom_tat = []
    t0 = time.time()
    for i, d in enumerate(ds, 1):
        rel = os.path.relpath(d, GOC)
        if a.bo_qua_da_chay and not a.liet_ke and da_chay(d):
            tom_tat.append((rel, 'ĐÃ CÓ', '', [])); continue
        print(f'\n[{i}/{len(ds)}] {rel}', flush=True)
        t = time.time()
        try:
            kq, ghi_chu, tep = chay_bai(d, a)
        except Exception as e:                    # một bài lỗi không làm dừng cả đợt
            kq, ghi_chu, tep = 'LỖI', f'{type(e).__name__}: {e}', []
        tom_tat.append((rel, kq, ghi_chu, tep))
        if not a.liet_ke:
            print(f'  {kq} ({time.time() - t:.0f} s){" – " + ghi_chu if ghi_chu else ""}')
            for x in tep:
                print('   ', x)
    dem = {}
    for _, kq, _, _ in tom_tat:
        dem[kq] = dem.get(kq, 0) + 1
    dong = [f'Tóm tắt ({time.strftime("%Y-%m-%d %H:%M")}, {time.time() - t0:.0f} s): '
            + ', '.join(f'{k} {v}' for k, v in dem.items())]
    for rel, kq, ghi_chu, _ in tom_tat:
        if kq not in ('ĐÃ CHẠY', 'ĐÃ CÓ', 'LIỆT KÊ'):
            dong.append(f'  {kq:8} {rel}{" – " + ghi_chu if ghi_chu else ""}')
    print('\n' + '\n'.join(dong))
    if not a.liet_ke:
        tep = os.path.join(LOG_DIR, time.strftime('tom_tat_%Y%m%d_%H%M%S.txt'))
        open(tep, 'w', encoding='utf-8').write('\n'.join(dong) + '\n\n' + '\n'.join(
            f'{kq:8} {rel}' + ''.join(f'\n           {x}' for x in t) for rel, kq, _, t in tom_tat) + '\n')
        print(f'Đã ghi {os.path.relpath(tep, GOC)}')


if __name__ == '__main__':
    main()
