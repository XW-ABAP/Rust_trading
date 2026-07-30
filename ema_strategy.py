from nautilus_trader.trading.strategy import Strategy, StrategyConfig
from nautilus_trader.model.data import Bar, BarType
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.indicators import ExponentialMovingAverage

class EMACrossConfig(StrategyConfig):
    instrument_id: str = "BTCUSDT.BINANCE"
    bar_type: str = "BTCUSDT.BINANCE-1-MINUTE-LAST-EXTERNAL"
    fast_period: int = 10
    slow_period: int = 20
    trade_size: float = 0.1

class EMACrossStrategy(Strategy):
    def __init__(self, config: EMACrossConfig):
        super().__init__(config)
        self.instrument_id = InstrumentId.from_str(config.instrument_id)
        self.bar_type = BarType.from_str(config.bar_type)
        self.trade_size = config.trade_size
        self.fast_ema = ExponentialMovingAverage(config.fast_period)
        self.slow_ema = ExponentialMovingAverage(config.slow_period)
        self.cross_above = False
        self.cross_below = False

    def on_start(self):
        self.subscribe_bars(self.bar_type)
        self.log.info("🚀 双均线策略启动")

    def on_bar(self, bar: Bar):
        self.fast_ema.update(bar.close.as_double())
        self.slow_ema.update(bar.close.as_double())
        if not self.fast_ema.initialized or not self.slow_ema.initialized:
            return

        fast_val = self.fast_ema.value
        slow_val = self.slow_ema.value

        if fast_val > slow_val and not self.cross_above:
            self.cross_above = True
            self.cross_below = False
            self.flatten_position(self.instrument_id)
            self.submit_order(
                self.order_factory.market(
                    instrument_id=self.instrument_id,
                    order_side=OrderSide.BUY,
                    quantity=self.instrument.make_qty(self.trade_size),
                )
            )
            self.log.info(f"📈 金叉买入 {self.trade_size}")

        elif fast_val < slow_val and not self.cross_below:
            self.cross_below = True
            self.cross_above = False
            self.flatten_position(self.instrument_id)
            self.submit_order(
                self.order_factory.market(
                    instrument_id=self.instrument_id,
                    order_side=OrderSide.SELL,
                    quantity=self.instrument.make_qty(self.trade_size),
                )
            )
            self.log.info(f"📉 死叉卖出 {self.trade_size}")

    @property
    def instrument(self):
        return self.cache.instrument(self.instrument_id)
