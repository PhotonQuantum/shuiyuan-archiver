# 水源社区存档工具

一个将上海交通大学[水源社区](https://shuiyuan.sjtu.edu.cn)的文章存档的工具。

<img src="screenshots/1.png" alt="screenshot_2" style="zoom:50%;" />

## 使用方法

1. 运行下载的 exe (Windows) 或 dmg (Mac) 文件
> 如果系统询问您是否要运行程序，请选择允许。

2. 在弹出的浏览器窗口中登录水源论坛，选择授权，并将授权代码按 Ctrl+V 粘贴到程序中。

3. 将要下载的贴子的完整地址粘贴到输入框中，选择保存位置，然后点击下载。

贴子将以 页码.html 的文件名存储。注意在移动存档时，请务必保留 resources 文件夹。

## 疑难解答

### 无法在程序里粘贴文字

右键点击是无效的，请使用 Ctrl+C 和 Ctrl+V 来粘贴。

### 粘贴授权码到程序中没有反应，或者授权码残缺

显示问题，其实内容已经成功粘贴，点登录即可。

### 程序界面假死不动，但是任务管理器中却没有显示无响应 (Windows)

可能是显示引擎在 Windows 上的 Bug，如果假死前点击了下载，那么程序已经在后台开始工作，可以稍等之后在输出文件夹查看是否已经下载完成。

### 程序闪退无法打开 (Windows)

请检查你的显卡驱动是否是最新的。过老或者不支持的显卡驱动会导致 OpenGL 引擎无法初始化。

## 许可

本项目遵循 MIT 协议。详情请参见 [LICENSE](LICENSE.txt)。以下文本为节选译注，仅英文原文有法律效力。

> 本软件是“如此”提供的，没有任何形式的明示或暗示的保证，包括但不限于对适销性、特定用途的适用性和不侵权的保证。
> 在任何情况下，作者或版权持有人都不对任何索赔、损害或其他责任负责，
> 无论这些追责来自合同、侵权或其它行为中，还是产生于、源于或有关于本软件以及本软件的使用或其它处置。

# Archiver for Shuiyuan BBS

A tool to archive an article on [Shuiyuan BBS](https://shuiyuan.sjtu.edu.cn) of SJTU.

## Usage

1. Run the downloaded exe (Windows) or dmg (Mac) file.
> If the system asks you whether to run the program, please select "Allow".

2. Login to the Shuiyuan BBS, click "Authorize" and paste the authorization code into the program.

3. Paste the full URL of the article you want to download into the input box,
select the location to save, and click "下载"(download).

The article will be saved as "{page}.html". Note that when moving the archive, do not delete the "resources" folder.

## Troubleshooting

### Cannot paste text in the program

Please use Ctrl+C and Ctrl+V to paste.

### Text pasted into the program is broken

This is a display issue. Just ignore it and continue.

### The program is frozen but the task manager shows it's still responding (Windows)

If the program is frozen before you click the download button, the program has already started working in the background.
You can wait for a while and then check the output folder to see if the article has been downloaded.

### The program crashes and won't open (Windows)

Please check your video card driver is up-to-date.
Older or unsupported video card drivers may cause OpenGL engine to fail.

## License

This project is licensed under the [MIT License](LICENSE.txt).