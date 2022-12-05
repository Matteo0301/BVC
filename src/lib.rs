#[macro_use]
mod log_formatter;

pub mod BVC {
    use chrono::Utc;
    use std::{
        cell::RefCell,
        collections::HashMap,
        fs::{File, OpenOptions},
        io::prelude::*,
        rc::Rc,
    };
    use unitn_market_2022::{
        event::{event::Event, notifiable::Notifiable},
        good::{self, consts::STARTING_CAPITAL, good::Good, good_kind::GoodKind},
        market::{
            good_label::GoodLabel, BuyError, LockBuyError, LockSellError, Market,
            MarketGetterError, SellError,
        },
    };
    use TimeEnabler::{Skip, Use};

    use rand::thread_rng;
    use rand::Rng;

    const NAME: &'static str = "BVC";
    const MAX_LOCK_TIME: u64 = 12;
    const MAX_LOCK_BUY_NUM: u8 = 4;
    const MAX_LOCK_SELL_NUM: u8 = 4;
    const LOWER_EUR_INIT_BOUND_PERCENTAGE: f32 = 0.3;
    const UPPER_EUR_INIT_BOUND_PERCENTAGE: f32 = 0.4;
    const LOWER_GOODS_INIT_BOUND_PERCENTAGE: f32 = 0.35;
    const UPPER_GOODS_INIT_BOUND_PERCENTAGE: f32 = 0.45;

    pub struct BVCMarket {
        /* eur: Good,
        usd: Good,
        yen: Good,
        yuan: Good, */
        time: u64, // needs to be reset before reaching U64::MAX and change transaction times accordingly
        oldest_lock_buy_time: TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        oldest_lock_sell_time: TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        good_data: HashMap<GoodKind, GoodInfo>,
        buy_locks: HashMap<String, LockBuyGood>,
        sell_locks: HashMap<String, LockSellGood>,
        active_buy_locks: u8,
        active_sell_locks: u8,
        subscribers: Vec<Box<dyn Notifiable>>,
        log_file: File,
    }

    enum TimeEnabler {
        Use(u64, String),
        Skip,
    }

    struct GoodInfo {
        info: Good,
        buy_exchange_rate: f32,
        sell_exchange_rate: f32,
    }

    struct LockBuyGood {
        locked_good: Good,
        buy_price: f32,
        lock_time: u64,
    }

    struct LockSellGood {
        locked_quantity: f32,
        sell_price: f32,
        lock_time: u64,
    }

    impl BVCMarket {
        fn write_on_log_file(&mut self, log_str: String) {
            write!(self.log_file, "{}", log_str);
        }
        fn update_locks(&mut self) {
            //remove buy locks
            match self.oldest_lock_buy_time {
                Use(oldest, trader) if oldest + MAX_LOCK_TIME < self.time => {
                    let lock = self.buy_locks.get(&trader).unwrap();
                    let good = self
                        .good_data
                        .get_mut(&lock.locked_good.get_kind())
                        .unwrap();
                    good.info.merge(lock.locked_good);
                    self.buy_locks.remove(&trader);
                    self.active_buy_locks -= 1;
                    self.oldest_lock_buy_time = if self.buy_locks.len() == 0 {
                        Skip
                    } else {
                        let mut oldest = Use(self.time, String::new());
                        for (trader, good) in self.buy_locks {
                            oldest = match oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, trader)
                                }
                                other => oldest,
                            }
                        }
                        oldest
                    }
                }
                other => (),
            }

            //remove sell locks
            match self.oldest_lock_sell_time {
                Use(oldest, trader) if oldest + MAX_LOCK_TIME < self.time => {
                    self.sell_locks.remove(&trader);
                    self.active_sell_locks -= 1;
                    self.oldest_lock_sell_time = if self.sell_locks.len() == 0 {
                        Skip
                    } else {
                        let mut oldest = Use(self.time, String::new());
                        for (trader, good) in self.sell_locks {
                            oldest = match oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, trader)
                                }
                                other => oldest,
                            }
                        }
                        oldest
                    }
                }
                other => (),
            }

            return;
        }

        fn increment_time(&mut self) {
            self.time += 1;
            if self.time == std::u64::MAX {
                let oldest_lock_buy = match self.oldest_lock_buy_time {
                    Skip => std::u64::MAX,
                    Use(time, _) => time,
                };
                let oldest_lock_sell = match self.oldest_lock_sell_time {
                    Skip => std::u64::MAX,
                    Use(time, _) => time,
                };
                let oldest = std::cmp::min(oldest_lock_buy, oldest_lock_sell);
                if oldest != std::u64::MAX {
                    self.oldest_lock_buy_time = match self.oldest_lock_buy_time {
                        Use(time, trader) => Use(time - oldest, trader),
                        Skip => Skip,
                    };
                    self.oldest_lock_sell_time = match self.oldest_lock_sell_time {
                        Use(time, trader) => Use(time - oldest, trader),
                        Skip => Skip,
                    };
                    for (trader, good) in self.buy_locks {
                        good.lock_time -= oldest;
                    }
                    for (trader, good) in self.sell_locks {
                        good.lock_time -= oldest;
                    }
                    self.time = self.time - oldest + 1;
                }
            }
            self.update_locks();
        }

        //to implement fluctuation of the goods
        fn fluctuate_quantity(&mut self) {}

        //to update the prices according to market rules
        fn update_prices(&mut self) {}

        //to notify other markets
        fn notify_markets(&mut self, event: Event) {
            for m in self.subscribers {
                m.on_event(event)
            }
        }

        fn token(operation: String, trader: String, time: u64) -> String {
            format!("{}-{}-{}", operation, trader, time)
        }
    }

    impl Notifiable for BVCMarket {
        fn add_subscriber(&mut self, subscriber: Box<dyn Notifiable>) {
            self.subscribers.push(subscriber);
        }
        fn on_event(&mut self, event: Event) {}
    }

    impl Market for BVCMarket {
        fn new_random() -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
            let mut max = STARTING_CAPITAL;
            let (mut eur, mut yen, mut usd, mut yuan): (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 0.0);
            let mut rng = thread_rng();
            eur = rng.gen_range(
                max * LOWER_EUR_INIT_BOUND_PERCENTAGE,
                max * UPPER_EUR_INIT_BOUND_PERCENTAGE,
            );
            max -= eur;
            yen = rng.gen_range(
                max * LOWER_GOODS_INIT_BOUND_PERCENTAGE,
                max * UPPER_GOODS_INIT_BOUND_PERCENTAGE,
            );
            max -= yen;
            usd = rng.gen_range(
                max * LOWER_GOODS_INIT_BOUND_PERCENTAGE,
                max * UPPER_GOODS_INIT_BOUND_PERCENTAGE,
            );
            max -= usd;
            yuan = max;
            Self::new_with_quantities(eur, yen, usd, yuan)
        }

        fn new_with_quantities(eur: f32, yen: f32, usd: f32, yuan: f32) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
            //Initialize prices tbd
            //log_market_init!(..);
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(format!("log_{}.txt", NAME))
                .expect("Unable to create log file !");
            let m: BVCMarket = BVCMarket {
                /* eur: Good {
                    kind: GoodKind::EUR,
                    quantity: eur,
                },
                yen: Good {
                    kind: GoodKind::YEN,
                    quantity: yen,
                },
                usd: Good {
                    kind: GoodKind::USD,
                    quantity: usd,
                },
                yuan: Good {
                    kind: GoodKind::YUAN,
                    quantity: yuan,
                }, */
                time: 0,
                oldest_lock_buy_time: Skip,
                oldest_lock_sell_time: Skip,
                active_buy_locks: 0,
                active_sell_locks: 0,
                good_data: HashMap::new(), // Implement starting prices here
                buy_locks: HashMap::new(),
                sell_locks: HashMap::new(),
                subscribers: Vec::new(),
                log_file: file,
            };
            Rc::new(RefCell::new(m))
        }

        fn new_file(path: &str) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
            Self::new_random()
        }

        fn get_name(&self) -> &'static str {
            NAME
        }

        fn get_budget(&self) -> f32 {
            self.good_data[&GoodKind::EUR].info.get_qty()
        }

        fn get_buy_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {
            if quantity.is_sign_negative() {
                return Err(MarketGetterError::NonPositiveQuantityAsked);
            }
            // Discounts to be implemented, i.e.: if the trader wants to buy more than 50% of the market good, i will apply a scalable discount
            // Remember if the kind is EUROS we must change it 1:1
            match kind {
                GoodKind::EUR => Ok(quantity),
                _ => Ok(self.good_data[&kind].buy_exchange_rate * quantity),
            }
        }

        fn get_sell_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {
            if quantity.is_sign_negative() {
                return Err(MarketGetterError::NonPositiveQuantityAsked);
            }
            // Discounts to be implemented, i.e.: if the trader wants to sell more than 50% of the good (percentage relative to the quantity of the Market not the Trader's one) i will apply a (less) scalable discount
            // Remember if the kind is EUROS we must change it 1:1
            match kind {
                GoodKind::EUR => Ok(quantity),
                _ => Ok(self.good_data[&kind].sell_exchange_rate * quantity),
            }
        }

        fn get_goods(&self) -> Vec<GoodLabel> {
            let prices: Vec<GoodLabel> = Vec::new();
            for (kind, data) in self.good_data {
                prices.push(GoodLabel {
                    good_kind: kind, // Does this copy or move ??
                    quantity: data.info.get_qty(),
                    exchange_rate_buy: data.buy_exchange_rate,
                    exchange_rate_sell: data.sell_exchange_rate,
                });
            }
            prices
        }

        fn lock_buy(
            &mut self,
            kind_to_buy: GoodKind,
            quantity_to_buy: f32,
            bid: f32,
            trader_name: String,
        ) -> Result<String, LockBuyError> {
        }

        fn buy(&mut self, token: String, cash: &mut Good) -> Result<Good, BuyError> {}

        fn lock_sell(
            &mut self,
            kind_to_sell: GoodKind,
            quantity_to_sell: f32,
            offer: f32,
            trader_name: String,
        ) -> Result<String, LockSellError> {
            let mut token: String;
            //negative quantity
            if quantity_to_sell < 0.0 {
                return Err(LockSellError::NonPositiveQuantityToSell {
                    negative_quantity_to_sell: quantity_to_sell,
                });
            }

            //non positive offer
            if offer <= 0.0 {
                return Err(LockSellError::NonPositiveOffer {
                    negative_offer: offer,
                });
            }

            //max lock reached
            if self.active_sell_locks == MAX_LOCK_SELL_NUM {
                return Err(LockSellError::MaxAllowedLocksReached);
            }

            return Ok(token);
        }

        fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {}
    }
}
