//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

use bsp::entry;
use defmt_rtt as _;
use panic_probe as _;
use defmt::info;

// Provide an alias for our BSP so we can switch targets quickly.
// Uncomment the BSP you included in Cargo.toml, the rest of the code does not need to change.
use rp_pico as bsp;
use embedded_graphics::Drawable;
use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::text::Text;
use rp2040_hal::Clock;
use rp2040_hal::clocks::init_clocks_and_plls;
use rp2040_hal::fugit::RateExtU32;
use rp2040_hal::gpio::{FunctionI2C,  Pins, PullUp};
use rp2040_hal::i2c::I2C;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use ssd1306::prelude::{DisplayConfig, DisplayRotation, DisplaySize128x64};


// #[defmt::panic_handler]
// fn panic() -> ! {
//     error!("Panic occurred!");
//     loop {}
// }

#[entry]
fn main() -> ! {
    info!("Program start");
    // 'pac'是一个模块,通常由外设访问层(Peripheral Access Crate)生成工具自动生成.
    // pac 模块包含了对微控制器所有外设的定义和访问接口。所以我们后续要使用I2C连接OLED显示屏的时候就是一种外设控制，所以要使用pac模块
    // 至于Peripherals 是一个代表微控制器外设的结构体，在该代码中则代表rp2040的外设。take方法则是一种实例化方式。
    // unwrap是rust的特殊语法专门用于处理Opinion类型的，我们下面则完整的处理Opinion，不使用unwrap;
    let mut pac = rp2040_hal::pac::Peripherals::take().unwrap();
    // 实际上上述操作主要做了两件事 1. 获取RP2040的所有外设权限 2. 初始化一次RP2040的外设结构体实例，并且保证只初始化一次

    // 获取RP2040的看门狗定时器。那么什么是看门狗定时器？
    // 看门狗定时器是一种硬件的计时器，用于监测和恢复系统中的故障。看门狗定时器通常在嵌入式系统中用来放置系统死锁或者长时间未响应。
    // 下面的代码则是利用RP2040的外设实例中的看门狗寄存器去初始化看门狗的实例
    // 具体在硬件层面我们看两个步骤
    // 1. pac.WATCHDOG 代表寄存器块，该块包含了控制看门狗定时器的各种寄存器
    // 2. Watchdog::new() 是一个初始化过程，主要包括设置初始的计数值,配置看门狗的超时行为等
    let mut watchdog = rp2040_hal::watchdog::Watchdog::new(pac.WATCHDOG);
    // 那么问题来了？ watchdog 是怎么解决系统死锁和系统恢复的呢？
    // 实际上我们有一个“喂狗”的概念在里面，可以初步理解为通过一个赋值的动作代表喂狗，如果某个超时时间内没有触发喂狗操作，则认为死锁。于是触发复位。
    // 所以我们系统运行的过程中要喂狗----喂狗----喂狗

    // Sio 是RP2040芯片中 单周期I/O (Single-Cycle I/O)的结构体，
    // sio提供了一些常用的功能模块，比如GPIO控制、内部计时器等。这些模块通常可以更快速的访问硬件资源
    // 最简单的理解就是如果在RP2040中想操作GPIO引脚，使用SIO寄存器就对了
    // SIO 可以做很多事比如1. GPIO控制 2. 内部计时器 3. 互斥和锁
    let sio = rp2040_hal::sio::Sio::new(pac.SIO);

    // 我们后续的操作要利用到GPIO引脚，而pin则代表了RP2040上面的GPIO引脚的配置，而下面的参数我将一一介绍
    // 1. IO_BANK0 是控制虽有GPIO引脚的输入输出功能、中断配置等
    // 2. PADS_BANK0 是控制GPIO引脚的电气特性。可以配置每个模块的上拉电阻、下拉电阻、驱动强度等。比如之前我们使用按钮模块的时候需要上啦或者下拉电阻就要利用这个特性
    // 3. gpio_bank0 是提供gpio的访问和控制的授权
    // 4. RESETS 是用于复位和初始化外设，使他们的状态处于已知状态.首先确保外设处于复位状态，然后接触复位开始正常工作
    let pins = Pins::new(pac.IO_BANK0,pac.PADS_BANK0,sio.gpio_bank0,& mut pac.RESETS);
    // 综上所属Pins::new 的初始化是整合了针对GPIO的通用操作的权限，保证后续针对GPIO的操作都可以使用pins去控制

    // scl 和sda 是I2C协议中的两根线(剩下两根是VCC和GND)
    // scl 代表串行时钟线，由主设备生成的时钟信号，用于同步数据传输
    // sda 代表串行数据线，用于传输数据，主设备和从设备共用这条线
    // 流程是主设备通过SCL生成时钟信号，通过SDA发送或者接受数据。多种设备共享这两条线的时候通过设备地址进行区分

    let scl_pin  = pins.gpio5.reconfigure::<FunctionI2C,PullUp>();
    let sda_pin = pins.gpio4.reconfigure::<FunctionI2C,PullUp>();

    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
        .ok()
        .unwrap();
    // 实际上开始初始化I2C外设
    let i2c = I2C::i2c0(
        pac.I2C0,
        sda_pin,
        scl_pin,
        400.kHz(), // 指定时钟频率为400.kHZ
        &mut pac.RESETS,
        clocks.system_clock.freq(),
    );

    // 这行就是用上面的i2c去初始化我们显示屏显示的interface
    // I2cDisplayInterface 是一个抽象的I2C显示的接口
    // 有许多显示的协议支持这个接口，比如下面要用的ssd1306
    let interface = I2CDisplayInterface::new(i2c);

    // 利用ssd1306包操作OLED，初始化出display对象
    // rotate0 代表初始化旋转式0度
    // size 是128 * 64 的像素
    let mut display = Ssd1306::new(interface,DisplaySize128x64,DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    // 初始化显示屏操作
    display.init().unwrap();

    let text_style = MonoTextStyle::new(&FONT_6X10,BinaryColor::On);

    // 设置display的图画功能，分别是文案是hello,world。位置是 (0,10)，文字的样式是6 * 10的大小、白色展示
    Text::new("hello,world",Point::new(0,20),text_style)
        .draw(&mut display).unwrap();

    // flush生效显示屏内容显示
    display.flush().unwrap();

    // 这行就需要好好理解了
    // 下面需要做的事是让微控制器进入低功耗状态，直到发生中断为止。具体来说它使用了ARM-Cortex-M 内核的wfi(Wait-for-Interrupt)指令
    // 节能啊
    loop {
        cortex_m::asm::wfi();
    }
}

// End of file
