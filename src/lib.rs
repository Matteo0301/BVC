#![allow(non_snake_case)]

#[macro_use]
mod log_formatter;

pub mod BVC {
    use core::panic;
    use chrono::Utc;
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet},
        //fmt::Error,
        fs::{File, OpenOptions},
        io::prelude::*,
        rc::Rc,
    };
    use unitn_market_2022::{
        event::{
            event::{Event, EventKind},
            notifiable::Notifiable,
        },
        good::{consts::{STARTING_CAPITAL, DEFAULT_EUR_USD_EXCHANGE_RATE, DEFAULT_EUR_YEN_EXCHANGE_RATE, DEFAULT_EUR_YUAN_EXCHANGE_RATE}, good::Good, good_kind::GoodKind},
        market::{
            good_label::GoodLabel, BuyError, LockBuyError, LockSellError, Market,
            MarketGetterError, SellError,
        },
    };
    use TimeEnabler::{Skip, Use};

    use rand::{thread_rng, seq::SliceRandom};
    use rand::Rng;

    const NAME: &'static str = "BVC";

    //Locks and other costraint constants
    const MAX_LOCK_TIME: u64 = 12;
    const MAX_LOCK_BUY_NUM: u8 = 4;
    const MAX_LOCK_SELL_NUM: u8 = 4;
    const MINIMUM_GOOD_QUANTITY_PERCENTAGE: f32 = 0.25;
    const MINIMUM_EUR_QUANTITY_PERCENTAGE: f32 = 0.20;
    const BUY_TO_SELL_PERCENTAGE: f32 = 0.93;

    //Good initialization constants
    const EUR_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.25;
    const EUR_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.35;
    const SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.30;
    const SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.36;
    const THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.45;
    const THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.55;

    //Reverse exchange rates constants
    const DEFAULT_USD_EUR_EXCHANGE_RATE : f32 = 1.0/DEFAULT_EUR_USD_EXCHANGE_RATE;
    const DEFAULT_YEN_EUR_EXCHANGE_RATE : f32 = 1.0/DEFAULT_EUR_YEN_EXCHANGE_RATE;
    const DEFAULT_YUAN_EUR_EXCHANGE_RATE : f32 = 1.0/DEFAULT_EUR_YUAN_EXCHANGE_RATE;

    //Quantity bounds and price discount constants to apply different price schemes + Lock buy quantity discounts
    const MAX_INFLATION_PRICE_INCREASE_PERCENTAGE : f32 = 0.1;
    const DEFAULT_PRICE_LOWER_BOUND_QTY_PERCENTAGE : f32 = 1.0;
    const FIRST_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE : f32 = 1.05;
    const FIRST_DEFLATION_PRICE_DISCOUNT : f32 = 0.98;
    const SECOND_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE : f32 = 1.10;
    const SECOND_DEFLATION_PRICE_DISCOUNT : f32 = 0.975;
    const THIRD_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE : f32 = 1.30;
    const THIRD_DEFLATION_PRICE_DISCOUNT : f32 = 0.97;
    const MAX_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE : f32 = 1.60;
    const MAX_DEFLATION_PRICE_DISCOUNT : f32 = 0.965;
    const FIRST_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY : f32 = 0.25;
    const FIRST_LOCK_BUY_DISCOUNT : f32 = 0.99;
    const SECOND_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY : f32 = 0.30;
    const SECOND_LOCK_BUY_DISCOUNT : f32 = 0.985;
    const THIRD_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY : f32 = 0.40;
    const THIRD_LOCK_BUY_DISCOUNT : f32 = 0.975;
    const MAX_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY : f32 = 0.50;
    const MAX_LOCK_BUY_DISCOUNT : f32 = 0.965;
    
    pub struct BVCMarket {
        time: u64, // needs to be reset before reaching U64::MAX and change transaction times accordingly
        oldest_lock_buy_time: TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        oldest_lock_sell_time: TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        mean: f32,
        active_buy_locks: u8,
        active_sell_locks: u8,
        good_data: HashMap<GoodKind, GoodInfo>,
        buy_locks: HashMap<String, LockBuyGood>,
        sell_locks: HashMap<String, LockSellGood>,
        subscribers: Vec<Box<dyn Notifiable>>,
        expired_tokens: HashSet<String>,
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
        initialization_qty: f32,
    }

    struct LockBuyGood {
        locked_good: Good,
        buy_price: f32,
        lock_time: u64,
    }

    #[derive(Clone)]
    struct LockSellGood {
        locked_eur: Good,
        receiving_good_qty: f32,
        locked_kind: GoodKind,
        lock_time: u64,
    }

    impl BVCMarket {

        fn write_on_log_file(&mut self, log_str: String) {
            match write!(self.log_file, "{}", log_str) {
                Ok(_) => (),
                Err(e) => panic!("Write to file error: {}\n",e)
            }
        }

        fn update_locks(&mut self) {
            let mut update_kind_price : Option<GoodKind> = None;

            // * Remove buy locks
            match &self.oldest_lock_buy_time {
                Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                    let lock = self.buy_locks.get(token).unwrap();
                    let good = self
                        .good_data
                        .get_mut(&lock.locked_good.get_kind())
                        .unwrap();
                    match good.info.merge(lock.locked_good.clone()) {
                        Ok(_) => (),
                        Err(e) => panic!("Different kind of goods in merge attempt @update_locks, details: {:?}",e),
                    }
                    update_kind_price = Some(lock.locked_good.get_kind());
                    self.buy_locks.remove(token);
                    self.expired_tokens.insert(token.clone());
                    self.active_buy_locks -= 1;
                    self.oldest_lock_buy_time = if self.buy_locks.len() == 0 {
                        Skip
                    } else {
                        let mut new_oldest = Use(self.time, String::new());
                        for (token, good) in &self.buy_locks {
                            new_oldest = match new_oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, token.clone())
                                }
                                _ => panic!("Should not return a Skip value"),
                            }
                        }
                        new_oldest
                    }
                }
                _ => (),
            }

            update_kind_price = match update_kind_price{
                Some(kind) => {
                    self.update_good_price(kind); 
                    None
                },
                _ => None,
            };

            // * Remove sell locks
            match &self.oldest_lock_sell_time {
                Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                    let lock = self.sell_locks.get(token).unwrap();
                    let good = self.good_data.get_mut(&GoodKind::EUR).unwrap();
                    match good.info.merge(lock.locked_eur.clone()) {
                        Ok(_) => (),
                        Err(e) => panic!("Different kind of goods in merge attempt @update_locks, details: {:?}",e),
                    }

                    update_kind_price = Some(lock.locked_eur.get_kind());
                    self.sell_locks.remove(token);
                    self.expired_tokens.insert(token.clone());
                    self.active_sell_locks -= 1;
                    self.oldest_lock_sell_time = if self.sell_locks.len() == 0 {
                        Skip
                    } else {
                        let mut new_oldest = Use(self.time, String::new());
                        for (token, good) in &self.sell_locks {
                            new_oldest = match new_oldest {
                                Use(time, _) if good.lock_time < time => {
                                    Use(good.lock_time, token.clone())
                                }
                                other => other,
                            }
                        }
                        new_oldest
                    }
                }
                _ => (),
            }

            match update_kind_price{
                Some(kind) => self.update_good_price(kind),
                _ => (),
            };

            return;
        }

        fn increment_time(&mut self) {
            // * Managing the time overflow
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
                        Skip => panic!("Matching of this enum should not return Skip!"),
                    };
                    self.oldest_lock_sell_time = match &self.oldest_lock_sell_time {
                        Use(time, token) => Use(time - oldest, token.clone()),
                        Skip => panic!("Matching of this enum should not return Skip!"),
                    };
                    for (_, good) in &mut self.buy_locks {
                        good.lock_time -= oldest;
                    }
                    for (_, good) in &mut self.sell_locks {
                        good.lock_time -= oldest;
                    }
                    self.time -= oldest;
                }
            }

            self.update_locks();
            self.time += 1;
            self.fluctuate_quantity();
        }

        //to implement fluctuation of the goods
        fn fluctuate_quantity(&mut self) {}

        fn update_good_price(&mut self, kind: GoodKind) {
            if let Some(good_info) = self.good_data.get_mut(&kind) {
                let good_qty = good_info.info.get_qty();

                let default_price = match kind{
                    GoodKind::USD => DEFAULT_EUR_USD_EXCHANGE_RATE,
                    GoodKind::YEN => DEFAULT_EUR_YEN_EXCHANGE_RATE,
                    GoodKind::YUAN => DEFAULT_EUR_YUAN_EXCHANGE_RATE,
                    GoodKind::EUR => panic!("Eur should not update its price !"),
                };

                if good_qty < self.mean * DEFAULT_PRICE_LOWER_BOUND_QTY_PERCENTAGE{
                    good_info.buy_exchange_rate = (((self.mean-good_qty)*MAX_INFLATION_PRICE_INCREASE_PERCENTAGE/
                                                   (self.mean*(1.0-MINIMUM_GOOD_QUANTITY_PERCENTAGE)))+1.0)*default_price;
                }else if good_qty < self.mean * FIRST_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE{
                    good_info.buy_exchange_rate = default_price;
                }else if good_qty < self.mean * SECOND_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE{
                    good_info.buy_exchange_rate = default_price*FIRST_DEFLATION_PRICE_DISCOUNT;
                }else if good_qty < self.mean * THIRD_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE{
                    good_info.buy_exchange_rate = default_price*SECOND_DEFLATION_PRICE_DISCOUNT;
                }else if good_qty < self.mean * MAX_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE{
                    good_info.buy_exchange_rate = default_price*THIRD_DEFLATION_PRICE_DISCOUNT;
                }else{
                    good_info.buy_exchange_rate = default_price*MAX_DEFLATION_PRICE_DISCOUNT;
                }
                good_info.sell_exchange_rate = good_info.buy_exchange_rate*BUY_TO_SELL_PERCENTAGE
            }else{
                panic!("Couldn't find GoodKind key {} in the good_data HashMap", kind)
            }
        }

        // * Notify other markets
        fn notify_markets(&mut self, event: Event) {
            for m in &mut self.subscribers {
                m.on_event(event.clone())
            }
            self.increment_time();
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
            self.increment_time();
            /* match event.kind {
                EventKind::Wait => self.increment_time(),
                _ => (),
            } */
        }
    }

    impl Market for BVCMarket {
        fn new_random() -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
            let mut max = STARTING_CAPITAL;
            let (mut eur, mut yen, mut usd, mut yuan): (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 0.0);
            let mut good_kinds = vec![GoodKind::USD,GoodKind::YUAN,GoodKind::YEN];
            let mut rng = thread_rng();
            
            eur = rng.gen_range(
                max * EUR_LOWER_BOUND_INIT_PERCENTAGE,
                max * EUR_UPPER_BOUND_INIT_PERCENTAGE,
            );
            max -= eur;

            good_kinds.shuffle(&mut rng);
            match good_kinds[0] {
                GoodKind::USD => {usd = rng.gen_range(
                    max * SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= usd },
                GoodKind::YEN => {yen = rng.gen_range(
                    max * SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= yen},
                GoodKind::YUAN => {yuan = rng.gen_range(
                    max * SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= yuan},
                GoodKind::EUR => panic!("Matched EUR which has been already initialized !")
            }
            
            match good_kinds[1] {
                GoodKind::USD => {usd = rng.gen_range(
                    max * THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= usd },
                GoodKind::YEN => {yen = rng.gen_range(
                    max * THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= yen},
                GoodKind::YUAN => {yuan = rng.gen_range(
                    max * THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
                    max * THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
                ); max -= yuan},
                GoodKind::EUR => panic!("Matched EUR which has been already initialized !")
            }

            match good_kinds[2] {
                GoodKind::USD => usd = max,
                GoodKind::YEN => yen = max,
                GoodKind::YUAN => yuan = max,
                GoodKind::EUR => panic!("Matched EUR which has been already initialized !")
            }

            Self::new_with_quantities(eur, yen*DEFAULT_EUR_YEN_EXCHANGE_RATE, usd*DEFAULT_EUR_USD_EXCHANGE_RATE, yuan*DEFAULT_EUR_YUAN_EXCHANGE_RATE)
        }

        fn new_with_quantities(eur: f32, yen: f32, usd: f32, yuan: f32) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(format!("log_{}.txt", NAME))
                .expect("Unable to create log file !");

            let mut market: BVCMarket = BVCMarket {
                time: 0,
                oldest_lock_buy_time: Skip,
                oldest_lock_sell_time: Skip,
                active_buy_locks: 0,
                active_sell_locks: 0,
                mean: (usd*DEFAULT_USD_EUR_EXCHANGE_RATE + yen*DEFAULT_YEN_EUR_EXCHANGE_RATE + yuan*DEFAULT_YUAN_EUR_EXCHANGE_RATE) / 3.0,
                good_data: HashMap::new(),
                buy_locks: HashMap::new(),
                sell_locks: HashMap::new(),
                subscribers: Vec::new(),
                log_file: file,
                expired_tokens: HashSet::new(),
            };

            market.good_data.insert(
                GoodKind::EUR,
                GoodInfo {
                    info: Good::new(GoodKind::EUR, eur),
                    buy_exchange_rate: 1.0,
                    sell_exchange_rate: 1.0,
                    initialization_qty: eur,
                },
            );

            market.good_data.insert(
                GoodKind::USD,
                GoodInfo {
                    info: Good::new(GoodKind::USD, usd),
                    buy_exchange_rate: 0.0,
                    sell_exchange_rate: 0.0,
                    initialization_qty: usd,
                },
            );

            market.good_data.insert(
                GoodKind::YEN,
                GoodInfo {
                    info: Good::new(GoodKind::YEN, yen),
                    buy_exchange_rate: 0.0,
                    sell_exchange_rate: 0.0,
                    initialization_qty: yen,
                },
            );

            market.good_data.insert(
                GoodKind::YUAN,
                GoodInfo {
                    info: Good::new(GoodKind::YUAN, yuan),
                    buy_exchange_rate: 0.0,
                    sell_exchange_rate: 0.0,
                    initialization_qty: yuan,
                },
            );

            market.update_good_price(GoodKind::USD);
            market.update_good_price(GoodKind::YEN);
            market.update_good_price(GoodKind::YUAN);
            market.write_on_log_file(log_format_market_init!(NAME,eur,usd,yen,yuan));

            Rc::new(RefCell::new(market))
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
                return Err(MarketGetterError::NonPositiveQuantityAsked)
            }
            
            let good_data = &self.good_data[&kind];

            // * Getting the quantity availability
            let available_good_qty = f32::min(good_data.initialization_qty*(1.0-MINIMUM_GOOD_QUANTITY_PERCENTAGE),
                                                good_data.info.get_qty());

            if quantity > available_good_qty {
                return Err(MarketGetterError::InsufficientGoodQuantityAvailable { 
                    requested_good_kind: kind, requested_good_quantity: quantity, available_good_quantity: available_good_qty 
                })
            }

            let mut good_price = good_data.buy_exchange_rate*quantity;
            
            // * Apply the discount
            if quantity >= FIRST_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY*available_good_qty {
                if quantity < SECOND_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY*available_good_qty{
                    good_price*=FIRST_LOCK_BUY_DISCOUNT;
                }else if quantity < THIRD_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY*available_good_qty{
                    good_price*=SECOND_LOCK_BUY_DISCOUNT;
                }else if quantity < MAX_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY*available_good_qty{
                    good_price*=THIRD_LOCK_BUY_DISCOUNT;
                }else{
                    good_price*=MAX_LOCK_BUY_DISCOUNT;
                }
            }

            Ok(good_price)
        }

        fn get_sell_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {
            if quantity.is_sign_negative() {
                return Err(MarketGetterError::NonPositiveQuantityAsked);
            }

            let eur_data = &self.good_data[&GoodKind::EUR];

            // * Getting the quantity availability
            let available_good_qty = f32::min(eur_data.initialization_qty*(1.0-MINIMUM_EUR_QUANTITY_PERCENTAGE),
                                                eur_data.info.get_qty());

            if quantity > available_good_qty  {
                return Err(MarketGetterError::InsufficientGoodQuantityAvailable { 
                    requested_good_kind: GoodKind::EUR, requested_good_quantity: quantity, available_good_quantity: available_good_qty 
                })
            }

            Ok(self.good_data[&kind].sell_exchange_rate*quantity)
        }

        fn get_goods(&self) -> Vec<GoodLabel> {
            let mut prices: Vec<GoodLabel> = Vec::new();
            for (kind, data) in &self.good_data {
                prices.push(GoodLabel {
                    good_kind: *kind,
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

            let token: String;

            // * Retrieve the price and look for errors
            let good_price = match self.get_buy_price(kind_to_buy,quantity_to_buy){
                Ok(price) => price,
                Err(error) => {
                    self.write_on_log_file(log_format_lock_buy!(NAME,trader_name,kind_to_buy,quantity_to_buy,bid));
                    match error {
                        MarketGetterError::NonPositiveQuantityAsked => {
                            return Err(LockBuyError::NonPositiveQuantityToBuy { negative_quantity_to_buy: quantity_to_buy })
                        },
                        MarketGetterError::InsufficientGoodQuantityAvailable { requested_good_kind, requested_good_quantity, available_good_quantity } => {
                            return Err(LockBuyError::InsufficientGoodQuantityAvailable { requested_good_kind: requested_good_kind, requested_good_quantity: requested_good_quantity, available_good_quantity: available_good_quantity })
                        }
                    }
                }
            };

            // * Non positive bid
            if bid <= 0.0 {
                self.write_on_log_file(log_format_lock_buy!(NAME,trader_name,kind_to_buy,quantity_to_buy,bid));
                return Err(LockBuyError::NonPositiveBid { negative_bid: bid });
            }

            // * Max locks reached
            if self.active_buy_locks == MAX_LOCK_BUY_NUM {
                self.write_on_log_file(log_format_lock_buy!(NAME,trader_name,kind_to_buy,quantity_to_buy,bid));
                return Err(LockBuyError::MaxAllowedLocksReached);
            }

            // * Bid too low
            if bid < good_price{
                self.write_on_log_file(log_format_lock_buy!(NAME,trader_name,kind_to_buy,quantity_to_buy,bid));
                return Err(LockBuyError::BidTooLow {
                    requested_good_kind: kind_to_buy,
                    requested_good_quantity: quantity_to_buy,
                    low_bid: bid,
                    lowest_acceptable_bid: good_price,
                });
            }

            // * Create a new buy transaction token
            token = BVCMarket::token(
                String::from("lock_buy"),
                trader_name.clone(),
                self.time,
            );
            
            // * Split the good, notify the markets and return the token
            if let Some(tmp) = self.good_data.get_mut(&kind_to_buy) {
                let good_splitted = tmp.info.split(quantity_to_buy).unwrap();
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
                
                self.increment_time();
                self.update_good_price(kind_to_buy);
                self.write_on_log_file(log_format_lock_buy!(NAME,trader_name,kind_to_buy,quantity_to_buy,bid,token));
                Ok(token)
            } else {
                panic!("Missing key: {} in good_data ", kind_to_buy)
            }
        }

        fn buy(&mut self, token: String, cash: &mut Good) -> Result<Good, BuyError> {

            if !self.buy_locks.contains_key(&token) {
                // * Check if the good has expired
                if self.expired_tokens.contains(&token) {
                    self.write_on_log_file(log_format_buy!(NAME,token,Err()));
                    return Err(BuyError::ExpiredToken {
                        expired_token: token,
                    });
                } else {
                    // * Otherwise it is an invalid token
                    self.write_on_log_file(log_format_buy!(NAME,token,Err()));
                    return Err(BuyError::UnrecognizedToken {
                        unrecognized_token: token,
                    });
                }
            }

            // * Invalid cash kind
            if cash.get_kind() != GoodKind::EUR {
                self.write_on_log_file(log_format_buy!(NAME,token,Err()));
                return Err(BuyError::GoodKindNotDefault {
                    non_default_good_kind: cash.get_kind(),
                });
            }

            // * Insufficient good quantity
            if self.buy_locks[&token].buy_price > cash.get_qty() {
                self.write_on_log_file(log_format_buy!(NAME,token,Err()));
                return Err(BuyError::InsufficientGoodQuantity {
                    contained_quantity: cash.get_qty(),
                    pre_agreed_quantity: self.buy_locks[&token].buy_price,
                });
            }


            // * Merge the eur good from the trader, notify other markets and return the locked good
            let eur_to_pay = self.buy_locks[&token].buy_price;
            let locked_good = self.buy_locks[&token].locked_good.clone(); // * There was a clone here
            self.buy_locks.remove(&token);
            self.active_buy_locks -= 1;
            if let Some(eur) = self.good_data.get_mut(&GoodKind::EUR) {
                eur.info.merge(cash.split(eur_to_pay).unwrap());
                self.notify_markets(Event {
                    kind: EventKind::Bought,
                    good_kind: locked_good.get_kind(),
                    quantity: locked_good.get_qty(),
                    price: eur_to_pay,
                });

                self.increment_time();
                self.write_on_log_file(log_format_buy!(NAME,token,Ok()));
                Ok(locked_good)
            } else {
                panic!("Missing key: GoodKind::EUR in good_data ")
            }
        }

        fn lock_sell(
            &mut self,
            kind_to_sell: GoodKind,
            quantity_to_sell: f32,
            offer: f32,
            trader_name: String,
        ) -> Result<String, LockSellError> {

            let token: String;

            // * Retrieve the price and look for errors
            let good_price = match self.get_sell_price(kind_to_sell, quantity_to_sell){
                Ok(price) => price,
                Err(error) => {
                    self.write_on_log_file(log_format_lock_sell!(NAME,trader_name,kind_to_sell,quantity_to_sell,offer));
                    match error {
                        MarketGetterError::NonPositiveQuantityAsked => {
                            return Err(LockSellError::NonPositiveQuantityToSell { negative_quantity_to_sell: quantity_to_sell })
                        },
                        MarketGetterError::InsufficientGoodQuantityAvailable { requested_good_kind, requested_good_quantity, available_good_quantity } => {
                            return Err(LockSellError::InsufficientDefaultGoodQuantityAvailable { offered_good_kind: requested_good_kind, offered_good_quantity: requested_good_quantity, available_good_quantity: available_good_quantity })
                        },
                    }
                }
            };

            // * Non positive offer
            if offer <= 0.0 {
                self.write_on_log_file(log_format_lock_sell!(NAME,trader_name,kind_to_sell,quantity_to_sell,offer));
                return Err(LockSellError::NonPositiveOffer {
                    negative_offer: offer,
                });
            }

            // * Max lock reached
            if self.active_sell_locks == MAX_LOCK_SELL_NUM {
                self.write_on_log_file(log_format_lock_sell!(NAME,trader_name,kind_to_sell,quantity_to_sell,offer));
                return Err(LockSellError::MaxAllowedLocksReached);
            }

            //* Offer too high
            if offer > good_price {
                self.write_on_log_file(log_format_lock_sell!(NAME,trader_name,kind_to_sell,quantity_to_sell,offer));
                return Err(LockSellError::OfferTooHigh {
                    offered_good_kind: kind_to_sell,
                    offered_good_quantity: quantity_to_sell,
                    high_offer: offer,
                    highest_acceptable_offer: good_price,
                });
            }

            token = BVCMarket::token(
                String::from("lock_sell"),
                trader_name.clone(),
                self.time,
            );
            
            //* Split the eur good, notify the markets and return the token
            if let Some(tmp) = self.good_data.get_mut(&GoodKind::EUR) {
                let eur_splitted = tmp.info.split(offer).unwrap();
                self.active_sell_locks += 1;
                self.sell_locks.insert(
                    token.clone(),
                    LockSellGood {
                        locked_eur: eur_splitted,
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
                
                self.increment_time();
                self.update_good_price(kind_to_sell);
                self.write_on_log_file(log_format_lock_sell!(NAME,trader_name,kind_to_sell,quantity_to_sell,offer,token));
                return Ok(token);
            } else {
                panic!("Missing key: GoodKind::EUR in good_data ")
            }
        }

        fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {

            if !self.sell_locks.contains_key(&token) {
                // * Check if the good has expired
                if self.expired_tokens.contains(&token) {
                    self.write_on_log_file(log_format_sell!(NAME,token,Err()));
                    return Err(SellError::ExpiredToken {
                        expired_token: token,
                    });
                } else {
                    // * Otherwise it is an invalid token
                    self.write_on_log_file(log_format_sell!(NAME,token,Err()));
                    return Err(SellError::UnrecognizedToken {
                        unrecognized_token: token,
                    });
                }
            }

            // * Kind of goods not matching
            if self.sell_locks[&token].locked_kind != good.get_kind() {
                self.write_on_log_file(log_format_sell!(NAME,token,Err()));
                return Err(SellError::WrongGoodKind {
                    wrong_good_kind: good.get_kind(),
                    pre_agreed_kind: self.sell_locks[&token].locked_kind,
                });
            }

            //* Quantity of goods not matching
            if good.get_qty() < self.sell_locks[&token].receiving_good_qty  {
                self.write_on_log_file(log_format_sell!(NAME,token,Err()));
                return Err(SellError::InsufficientGoodQuantity {
                    contained_quantity: good.get_qty(),
                    pre_agreed_quantity: self.sell_locks[&token].receiving_good_qty,
                });
            }

            // * Merge the good from the trader, notify other markets and return the locked eur pre agreed quantity
            let lock_info = self.sell_locks[&token].clone();
            self.sell_locks.remove(&token);
            self.active_sell_locks -= 1;
            if let Some(good_to_fill) = self.good_data.get_mut(&good.get_kind()) {
                
                good_to_fill.info.merge(good.split(lock_info.receiving_good_qty).unwrap());
    
                self.notify_markets(Event {
                    kind: EventKind::Sold,
                    good_kind: good.get_kind(),
                    quantity: lock_info.receiving_good_qty,
                    price: lock_info.locked_eur.get_qty(),
                });

                self.increment_time();
                self.write_on_log_file(log_format_sell!(NAME,token,Ok()));
                Ok(lock_info.locked_eur)
            } else {
                panic!("Missing key: {} in good_data ", good.get_kind())
            }
        }
    }
}
