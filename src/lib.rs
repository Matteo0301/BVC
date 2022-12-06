#[macro_use]
mod log_formatter;

pub mod BVC {
    //use chrono::Utc;
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet},
        //fmt::Error,
        fs::{File, OpenOptions},
        io::prelude::*,
        rc::Rc,
        str::FromStr,
    };
    use unitn_market_2022::{
        event::{
            event::{Event, EventKind},
            notifiable::Notifiable,
        },
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

    //constants for price changes
    const REFUSE_LOCK_BUY: f32 = 0.25;
    const REFUSE_LOCK_SELL: f32 = 0.20;
    const BUY_SELL_PERCENT: f32 = 1.1;

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
        expired_tokens: HashSet<String>,
        starting_prices: HashMap<GoodKind, f32>,
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
        locked_good: Good,
        receiving_good_qty: f32,
        locked_kind: GoodKind,
        lock_time: u64,
    }

    impl BVCMarket {
        fn write_on_log_file(&mut self, log_str: String) {
            write!(self.log_file, "{}", log_str);
        }
        fn update_locks(&mut self) {
            //remove buy locks
            match &self.oldest_lock_buy_time {
                Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                    let lock = self.buy_locks.get(token).unwrap();
                    let good = self
                        .good_data
                        .get_mut(&lock.locked_good.get_kind())
                        .unwrap();
                    good.info.merge(lock.locked_good.clone());
                    self.buy_locks.remove(token);
                    self.expired_tokens.insert(token.clone());
                    self.active_buy_locks -= 1;
                    self.oldest_lock_buy_time = if self.buy_locks.len() == 0 {
                        Skip
                    } else {
                        let mut oldest = Use(self.time, String::new());
                        for (token, good) in &self.buy_locks {
                            oldest = match oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, token.clone())
                                }
                                other => other,
                            }
                        }
                        oldest
                    }
                }
                _ => (),
            }

            //remove sell locks
            match &self.oldest_lock_sell_time {
                Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                    let lock = self.sell_locks.get(token).unwrap();
                    let good = self.good_data.get_mut(&GoodKind::EUR).unwrap();
                    good.info.merge(lock.locked_good.clone());

                    self.sell_locks.remove(token);
                    self.expired_tokens.insert(token.clone());
                    self.active_sell_locks -= 1;
                    self.oldest_lock_sell_time = if self.sell_locks.len() == 0 {
                        Skip
                    } else {
                        let mut oldest = Use(self.time, String::new());
                        for (token, good) in &self.sell_locks {
                            oldest = match oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, token.clone())
                                }
                                other => other,
                            }
                        }
                        oldest
                    }
                }
                _ => (),
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
                    self.oldest_lock_buy_time = match &self.oldest_lock_buy_time {
                        Use(time, token) => Use(time - oldest, token.clone()),
                        Skip => Skip,
                    };
                    self.oldest_lock_sell_time = match &self.oldest_lock_sell_time {
                        Use(time, token) => Use(time - oldest, token.clone()),
                        Skip => Skip,
                    };
                    for (_, good) in &mut self.buy_locks {
                        good.lock_time -= oldest;
                    }
                    for (_, good) in &mut self.sell_locks {
                        good.lock_time -= oldest;
                    }
                    self.time = self.time - oldest + 1;
                }
            }
            self.update_locks();
            self.update_prices();
            self.fluctuate_quantity();
        }

        //to implement fluctuation of the goods
        fn fluctuate_quantity(&mut self) {}

        fn initial_price(average: f32, qty: f32, default_price: f32) -> f32 {
            if qty < average {
                ((((average - qty) / average) * 0.50) + 1.0) * default_price
            } else if (qty - average) * 100.0 / average < 10.0 {
                default_price
            } else {
                default_price - default_price * 0.05
            }
        }

        fn update_kind_price(&mut self, kind: GoodKind, default_price: f32) {
            let average = (self.good_data[&GoodKind::USD].info.get_qty()
                + self.good_data[&GoodKind::YEN].info.get_qty()
                + self.good_data[&GoodKind::YUAN].info.get_qty())
                / 3.0;
            let qty = self.good_data[&kind].info.get_qty();
            let (buy, sell) = if qty < average {
                let tmp = ((((average - qty) / average) * 0.50) + 1.0) * default_price;
                (tmp, tmp * BUY_SELL_PERCENT)
            } else if (qty - average) / average < 0.1 {
                (default_price, default_price * BUY_SELL_PERCENT)
            } else if (qty - average) / average < 0.3 {
                (default_price * 0.99, default_price * 1.15)
            } else if (qty - average) / average < 0.6 {
                (default_price * 0.98, default_price * 1.25)
            } else if (qty - average) / average < 1.0 {
                (default_price * 0.97, default_price * 1.35)
            } else {
                (default_price * 0.95, default_price * 1.5)
            };

            if let Some(m) = self.good_data.get_mut(&kind) {
                m.buy_exchange_rate = buy;
                m.sell_exchange_rate = sell;
            };
        }

        //to update the prices according to market rules
        fn update_prices(&mut self) {
            self.update_kind_price(GoodKind::USD, good::consts::DEFAULT_EUR_USD_EXCHANGE_RATE);
            self.update_kind_price(GoodKind::YEN, good::consts::DEFAULT_EUR_YEN_EXCHANGE_RATE);
            self.update_kind_price(GoodKind::YUAN, good::consts::DEFAULT_EUR_YUAN_EXCHANGE_RATE);
        }

        fn max_offer(&mut self) -> f32 {
            self.good_data[&GoodKind::EUR].info.get_qty()
                - self.starting_prices[&GoodKind::EUR] * REFUSE_LOCK_SELL
        }

        fn min_bid(&mut self, kind: &GoodKind) -> f32 {
            self.good_data[kind].info.get_qty() - self.starting_prices[kind] * REFUSE_LOCK_SELL
        }

        //to notify other markets
        fn notify_markets(&mut self, event: Event) {
            for m in &mut self.subscribers {
                m.on_event(event.clone())
            }
            self.increment_time();
            //TODO implement logging here
            //self.write_on_log_file(String::new());
        }

        fn token(operation: String, trader: String, time: u64) -> String {
            format!("{}-{}-{}", operation, trader, time)
        }
    }

    impl Notifiable for BVCMarket {
        fn add_subscriber(&mut self, subscriber: Box<dyn Notifiable>) {
            self.subscribers.push(subscriber);
        }
        fn on_event(&mut self, event: Event) {
            match event.kind {
                EventKind::Wait => self.increment_time(),
                _ => (),
            }
        }
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
            let mut m: BVCMarket = BVCMarket {
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
                expired_tokens: HashSet::new(),
                starting_prices: HashMap::new(),
            };
            let average = (usd + yen + yuan) / 3.0;
            m.good_data.insert(
                GoodKind::EUR,
                GoodInfo {
                    info: Good::new(GoodKind::EUR, eur),
                    buy_exchange_rate: 1.0,
                    sell_exchange_rate: 1.0,
                },
            );

            let mut price =
                BVCMarket::initial_price(average, usd, good::consts::DEFAULT_EUR_USD_EXCHANGE_RATE);
            m.good_data.insert(
                GoodKind::USD,
                GoodInfo {
                    info: Good::new(GoodKind::USD, usd),
                    buy_exchange_rate: price,
                    sell_exchange_rate: price * BUY_SELL_PERCENT,
                },
            );

            let mut price =
                BVCMarket::initial_price(average, yen, good::consts::DEFAULT_EUR_YEN_EXCHANGE_RATE);
            m.good_data.insert(
                GoodKind::YEN,
                GoodInfo {
                    info: Good::new(GoodKind::YEN, yen),
                    buy_exchange_rate: price,
                    sell_exchange_rate: price * BUY_SELL_PERCENT,
                },
            );

            let mut price = BVCMarket::initial_price(
                average,
                yuan,
                good::consts::DEFAULT_EUR_YUAN_EXCHANGE_RATE,
            );
            m.good_data.insert(
                GoodKind::YUAN,
                GoodInfo {
                    info: Good::new(GoodKind::YUAN, yuan),
                    buy_exchange_rate: price,
                    sell_exchange_rate: price * BUY_SELL_PERCENT,
                },
            );

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
            let default_rate = match kind {
                GoodKind::EUR => 1.0,
                GoodKind::YEN => good::consts::DEFAULT_EUR_YEN_EXCHANGE_RATE,
                GoodKind::USD => good::consts::DEFAULT_EUR_USD_EXCHANGE_RATE,
                GoodKind::YUAN => good::consts::DEFAULT_EUR_YUAN_EXCHANGE_RATE,
            };
            let discount = if self.good_data[&kind].buy_exchange_rate > default_rate {
                let market_qty = self.good_data[&kind].info.get_qty();
                let buy_percent = quantity / market_qty;
                if buy_percent >= 0.3 && buy_percent < 0.5 {
                    0.05
                } else if buy_percent > 0.5 {
                    0.1
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let mut rate = self.good_data[&kind].buy_exchange_rate;
            rate = rate - rate * discount;
            Ok(rate * quantity)
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
            let mut prices: Vec<GoodLabel> = Vec::new();
            for (kind, data) in &self.good_data {
                prices.push(GoodLabel {
                    good_kind: kind.clone(), // Does this copy or move ??
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
            let mut token: String;
            //negative quantity
            if quantity_to_buy < 0.0 {
                //TODO log error
                return Err(LockBuyError::NonPositiveQuantityToBuy {
                    negative_quantity_to_buy: quantity_to_buy,
                });
            }

            //non positive bid
            if bid <= 0.0 {
                //TODO log error
                return Err(LockBuyError::NonPositiveBid { negative_bid: bid });
            }

            //max lock reached
            if self.active_buy_locks == MAX_LOCK_BUY_NUM {
                //TODO log error
                return Err(LockBuyError::MaxAllowedLocksReached);
            }

            //not enough goods
            if quantity_to_buy > self.good_data[&kind_to_buy].info.get_qty() {
                //TODO log error
                return Err(LockBuyError::InsufficientGoodQuantityAvailable {
                    requested_good_kind: kind_to_buy,
                    requested_good_quantity: quantity_to_buy,
                    available_good_quantity: self.good_data[&kind_to_buy].info.get_qty(),
                });
            }

            //bid too low
            if bid < self.min_bid(&kind_to_buy) {
                //TODO log error
                return Err(LockBuyError::BidTooLow {
                    requested_good_kind: kind_to_buy,
                    requested_good_quantity: quantity_to_buy,
                    low_bid: bid,
                    lowest_acceptable_bid: self.min_bid(&kind_to_buy),
                });
            }

            token = BVCMarket::token(
                //TODO log error
                String::from_str("lock_buy").unwrap(),
                trader_name,
                self.time,
            );

            if let Some(tmp) = self.good_data.get_mut(&kind_to_buy) {
                let mut good_splitted = tmp.info.split(quantity_to_buy).unwrap();
                self.active_buy_locks += 1;
                self.buy_locks.insert(
                    token.clone(),
                    LockBuyGood {
                        locked_good: good_splitted,
                        buy_price: bid,
                        lock_time: self.time,
                    },
                );

                self.notify_markets(Event {
                    kind: EventKind::LockedBuy,
                    good_kind: kind_to_buy,
                    quantity: quantity_to_buy,
                    price: bid,
                });
                Ok(token)
            } else {
                Err(LockBuyError::InsufficientGoodQuantityAvailable {
                    requested_good_kind: kind_to_buy,
                    requested_good_quantity: quantity_to_buy,
                    available_good_quantity: self.good_data[&kind_to_buy].info.get_qty(),
                })
            }
        }

        fn buy(&mut self, token: String, cash: &mut Good) -> Result<Good, BuyError> {
            if !self.buy_locks.contains_key(&token) {
                //expired token
                if self.expired_tokens.contains(&token) {
                    //TODO log error
                    return Err(BuyError::ExpiredToken {
                        expired_token: token,
                    });
                } else {
                    //invalid token
                    //TODO log error
                    return Err(BuyError::UnrecognizedToken {
                        unrecognized_token: token,
                    });
                }
            }

            //invalid cash kind
            if cash.get_kind() != GoodKind::EUR {
                //TODO log error
                return Err(BuyError::GoodKindNotDefault {
                    non_default_good_kind: cash.get_kind(),
                });
            }

            //insufficient good quantity
            if self.buy_locks[&token].buy_price > cash.get_qty() {
                //TODO log error
                return Err(BuyError::InsufficientGoodQuantity {
                    contained_quantity: cash.get_qty(),
                    pre_agreed_quantity: self.buy_locks[&token].buy_price,
                });
            }

            let received = self.buy_locks[&token].buy_price;
            let res = self.buy_locks[&token].locked_good.clone();
            self.buy_locks.remove(&token);
            self.active_buy_locks -= 1;
            if let Some(eur) = self.good_data.get_mut(&GoodKind::EUR) {
                eur.info.merge(cash.split(received).unwrap());
                self.notify_markets(Event {
                    kind: EventKind::Bought,
                    good_kind: res.get_kind(),
                    quantity: res.get_qty(),
                    price: received,
                });

                Ok(res)
            } else {
                panic!()
            }
        }

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
                //TODO log error
                return Err(LockSellError::NonPositiveQuantityToSell {
                    negative_quantity_to_sell: quantity_to_sell,
                });
            }

            //non positive offer
            if offer <= 0.0 {
                //TODO log error
                return Err(LockSellError::NonPositiveOffer {
                    negative_offer: offer,
                });
            }

            //max lock reached
            if self.active_sell_locks == MAX_LOCK_SELL_NUM {
                //TODO log error
                return Err(LockSellError::MaxAllowedLocksReached);
            }

            //not enough goods to sell
            if quantity_to_sell > self.good_data[&kind_to_sell].info.get_qty() {
                //TODO log error
                return Err(LockSellError::InsufficientDefaultGoodQuantityAvailable {
                    offered_good_kind: kind_to_sell,
                    offered_good_quantity: quantity_to_sell,
                    available_good_quantity: self.good_data[&kind_to_sell].info.get_qty(),
                });
            }

            //offer too high
            if offer > self.max_offer() {
                //TODO log error
                return Err(LockSellError::OfferTooHigh {
                    offered_good_kind: kind_to_sell,
                    offered_good_quantity: quantity_to_sell,
                    high_offer: offer,
                    highest_acceptable_offer: self.max_offer(),
                });
            }

            token = BVCMarket::token(
                //TODO log error
                String::from_str("lock_sell").unwrap(),
                trader_name,
                self.time,
            );

            if let Some(tmp) = self.good_data.get_mut(&GoodKind::EUR) {
                let eur_splitted = tmp.info.split(quantity_to_sell).unwrap();
                self.active_sell_locks += 1;
                self.sell_locks.insert(
                    token.clone(),
                    LockSellGood {
                        locked_good: eur_splitted,
                        receiving_good_qty: quantity_to_sell,
                        lock_time: self.time,
                        locked_kind: kind_to_sell,
                    },
                );

                self.notify_markets(Event {
                    kind: EventKind::LockedSell,
                    good_kind: kind_to_sell,
                    quantity: quantity_to_sell,
                    price: offer,
                });
                return Ok(token);
            } else {
            }

            let eur_splitted = self.good_data[&GoodKind::EUR]
                .info
                .clone()
                .split(quantity_to_sell)
                .unwrap();
            self.active_sell_locks += 1;
            self.sell_locks.insert(
                token.clone(),
                LockSellGood {
                    locked_good: eur_splitted,
                    receiving_good_qty: quantity_to_sell,
                    lock_time: self.time,
                    locked_kind: kind_to_sell,
                },
            );

            self.notify_markets(Event {
                kind: EventKind::LockedSell,
                good_kind: kind_to_sell,
                quantity: quantity_to_sell,
                price: offer,
            });
            return Ok(token);
        }

        fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {
            let mut res = Good::new(GoodKind::EUR, 0.0);

            if !self.sell_locks.contains_key(&token) {
                //expired token
                if self.expired_tokens.contains(&token) {
                    //TODO log error
                    return Err(SellError::ExpiredToken {
                        expired_token: token,
                    });
                } else {
                    //invalid token
                    //TODO log error
                    return Err(SellError::UnrecognizedToken {
                        unrecognized_token: token,
                    });
                }
            }

            //king of goods not matching
            if self.sell_locks[&token].locked_kind != good.get_kind() {
                //TODO log error
                return Err(SellError::WrongGoodKind {
                    wrong_good_kind: good.get_kind(),
                    pre_agreed_kind: self.sell_locks[&token].locked_kind,
                });
            }

            //quantity of goods not matching
            if self.sell_locks[&token].receiving_good_qty > good.get_qty() {
                //TODO log error
                return Err(SellError::InsufficientGoodQuantity {
                    contained_quantity: good.get_qty(),
                    pre_agreed_quantity: self.sell_locks[&token].receiving_good_qty,
                });
            }

            let price = self.sell_locks[&token].locked_good.get_qty();
            res.merge(self.sell_locks[&token].locked_good.clone());
            self.sell_locks.remove(&token);
            self.active_sell_locks -= 1;
            if let Some(tmp) = self.good_data.get_mut(&good.get_kind()) {
                let goodTmp = Good::new(good.get_kind(), good.get_qty());
                tmp.info.merge(goodTmp);

                self.notify_markets(Event {
                    kind: EventKind::Sold,
                    good_kind: good.get_kind(),
                    quantity: good.get_qty(),
                    price: price,
                });

                Ok(res)
            } else {
                panic!()
            }
        }
    }
}
