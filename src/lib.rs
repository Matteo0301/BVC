#![allow(non_snake_case)]
#![allow(unused_must_use)]
#![allow(unused_assignments)]

//!# BVC Market
//!
//!Here we will store the library files of our Market.
//!
//!## Market Strategy
//!
//!### Notation:
//!
//!- `buy price` := the price of the goods offered to the trader.
//!- `sell price` := the price of the goods that the market is willing to pay to the trader.
//!- `price` := exchange rate.
//!
//!## Initial goods allocation:
//!
//!> Note: our market must be initialized with **new_random()** any other use will probably cause unwanted behaviour.
//!
//!- `eur` := random percentage in range `[25%,35%)` of `STARTING_CAPITAL`.
//!- `second_good` := random percentage in range `[30%,36%)` of `(STARTING_CAPITAL - eur)`.
//!- `third_good` := random percentage in range `[45%,55%)` of `(STARTING_CAPITAL - eur - to_eur(second_good) )`.
//!- `fourth_good` := the remaining capital.
//!
//!> Note: In the initialization, good order is randomized.
//!
//!## Price fluctuation:
//!
//!> **Premise**: `eur` always has a 1:1 conversion rate
//!
//!The following rules are applied:  
//!
//!- The market refuses **any** `lock buy` that would leave it with less than `25%` of the initial quantity of the asked good.
//!
//!- The market refuses **any** `lock sell` that would leave it with less than `20%` of the initial `eur` quantity.
//!
//!- Define a value `mean` := for each good that is not EUR, convert it to EUR ( using default exchange rates ), sum them all together, then divide the obtained value by 3 ( which is the number of goods except EUR).  
//!
//!$$mean = \frac{\sum_{i=0}^{2}toEur(goods_i)}{3}$$
//!<br>
//!
//!- If a good has its quantity below the `mean` , (buy) price will fluctuate incrementally using this formula:
//!
//!$$ price = \left(\left(\left(1.0-\frac{toEur(goodQty)-(toEur(initialGoodQty)\cdot 0.25)}{mean-(toEur(initialGoodQty)\cdot 0.25)}\right)\cdot 0.1\right)+1.0\right)\cdot defaultPrice$$
//!<br>
//!
//!> This means that if a good it's at its minimum quantity, the trader will pay it 10% more than the default exchange rate.
//!
//!- If a good has its quantity between `[0%,5%)` over the `mean`, then the default price of that good will be used.
//!
//!- If a good overcomes the mean by a percentage in range `[5%,10%)`, a favorable price will be applied, hence it will deflate by `2%` from the default price.
//!
//!- If a good overcomes the mean by a percentage in range `[10%,30%)`, a favorable price will be applied, hence it will deflate by `2.5%` from the default price.
//!
//!- If a good overcomes the mean by a percentage in range `[30%,60%)`, a favorable price will be applied, hence it will deflate by `3%` from the default price.
//!
//!- If a good overcomes the mean by more than `60%`, a favorable price will be applied, hence it will deflate by `3.5%` from the default price.
//!
//!- The `sell price` is always lower than the `buy price`, by exactly `1%`.
//!
//!- If the trader wants to buy a quantity in range `[25%,30%)` of a certain good, the market will apply a `1%` discount on the `buy price` indiscriminately.
//!
//!- If the trader wants to buy a quantity in range `[30%,40%)` of a certain good, the market will apply a `1.5%` discount on the `buy price` indiscriminately.
//!
//!- If the trader wants to buy a quantity in range `[40%,50%)` of a certain good, the market will apply a `2.5%` discount on the `buy price` indiscriminately.
//!  
//!- If the trader wants to buy more than `50%` of a certain good, the market will apply a `3.5%` discount on the `buy price` indiscriminately.
//!
//!## Good conversion:
//!
//!The logic is trying to equalize good quantities, but not always, to avoid conflicts with the discount logic applyed in the price fluctuation.
//!
//!Every time the trader interacts with our or other markets by using `lock sell`, `lock buy`, `buy`, `sell`, there is a `10%` probability that the market will **try** to be rebalance its good quantities among the goods.  
//!
//!How ?
//!
//!- First compute this value:
//!  $$mean = \frac{\sum_{i=0}^{3}toEur(goods_i)}{4}$$
//!
//!- Then the market will look for two goods, one that is `suffering` which means it's the one that is worth the less converted to euros and it's not **exported**, and the `chosen good` that is worth the most converted to euros and such that:  
//!
//!  $$chosenGood = \max(toEur(good)\ \forall good :$$
//!  $$toEur(good) > toEur(sufferingGood) \wedge notImported(good)$$
//!  <br>
//!
//!  Mark the `suffering good` as **imported** and the `chosen good` as **exported** 
//!  > Note: The eur good will never be marked as Imported or Exported.
//!
//!  Then take a part of the latter, which is:
//!
//!  $$convertedPart = toSufferingGoodKind($$
//!  $$min(mean-toEur(sufferingGood),toEur(chosenGood)-mean))$$
//!  <br>
//!
//!  and then sum it to the `suffering` good.
//!
//!> Note: every 24 days the market will reset the Exported/Imported status for each good.
//!
//!## Event reaction
//!
//!To keep the market strategy safer and stable we decided to avoid reacting to external events generated by other markets.

#[macro_use]
mod log_formatter;

use chrono::Utc;
use core::panic;
use rand::Rng;
use rand::{seq::SliceRandom, thread_rng};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::prelude::*,
    rc::Rc,
};
use unitn_market_2022::{
    event::{
        event::{Event, EventKind},
        notifiable::Notifiable,
    },
    good::{
        consts::{
            DEFAULT_EUR_USD_EXCHANGE_RATE, DEFAULT_EUR_YEN_EXCHANGE_RATE,
            DEFAULT_EUR_YUAN_EXCHANGE_RATE, STARTING_CAPITAL,
        },
        good::Good,
        good_kind::GoodKind,
    },
    market::{
        good_label::GoodLabel, BuyError, LockBuyError, LockSellError, Market, MarketGetterError,
        SellError,
    },
};
use KindOfTrade::{Exported, Imported, Unknown};
use TimeEnabler::{Skip, Use};

const NAME: &'static str = "BVC";

//Locks and other costraint constants
const MAX_LOCK_TIME: u64 = 12;
const MAX_LOCK_BUY_NUM: u8 = 4;
const MAX_LOCK_SELL_NUM: u8 = 4;
const MINIMUM_GOOD_QUANTITY_PERCENTAGE: f32 = 0.25;
const MINIMUM_EUR_QUANTITY_PERCENTAGE: f32 = 0.20;
const BUY_TO_SELL_PERCENTAGE: f32 = 0.99;

//Good initialization constants
const EUR_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.25;
const EUR_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.35;
const SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.30;
const SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.36;
const THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE: f32 = 0.45;
const THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE: f32 = 0.55;

//Reverse exchange rates constants
const DEFAULT_USD_EUR_EXCHANGE_RATE: f32 = 1.0 / DEFAULT_EUR_USD_EXCHANGE_RATE;
const DEFAULT_YEN_EUR_EXCHANGE_RATE: f32 = 1.0 / DEFAULT_EUR_YEN_EXCHANGE_RATE;
const DEFAULT_YUAN_EUR_EXCHANGE_RATE: f32 = 1.0 / DEFAULT_EUR_YUAN_EXCHANGE_RATE;

//Quantity bounds and price discount constants to apply different price schemes + Lock buy quantity discounts
const MAX_INFLATION_PRICE_INCREASE_PERCENTAGE: f32 = 0.1;
const DEFAULT_PRICE_LOWER_BOUND_QTY_PERCENTAGE: f32 = 1.0;
const FIRST_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE: f32 = 1.05;
const FIRST_DEFLATION_PRICE_DISCOUNT: f32 = 0.98;
const SECOND_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE: f32 = 1.10;
const SECOND_DEFLATION_PRICE_DISCOUNT: f32 = 0.975;
const THIRD_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE: f32 = 1.30;
const THIRD_DEFLATION_PRICE_DISCOUNT: f32 = 0.97;
const MAX_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE: f32 = 1.60;
const MAX_DEFLATION_PRICE_DISCOUNT: f32 = 0.965;
const FIRST_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY: f32 = 0.25;
const FIRST_LOCK_BUY_DISCOUNT: f32 = 0.99;
const SECOND_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY: f32 = 0.30;
const SECOND_LOCK_BUY_DISCOUNT: f32 = 0.985;
const THIRD_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY: f32 = 0.40;
const THIRD_LOCK_BUY_DISCOUNT: f32 = 0.975;
const MAX_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY: f32 = 0.50;
const MAX_LOCK_BUY_DISCOUNT: f32 = 0.965;

//Good fluctuation constants
const PROBABILITY_OF_REBALANCE: f32 = 0.15;
const DURATION_OF_CHOSEN_KIND_OF_TRADE: u64 = 24;

//Debug related
const CHECK_IF_FLUCTUATION_OCCURS: bool = false;
const CHECK_IF_FIND_GOODS_TO_FLUCTUATE: bool = false;
const CHECK_IF_LOCK_BUY_DROPS: bool = false;
const CHECK_IF_LOCK_SELL_DROPS: bool = false;
const CHECK_BUY_PRICE: bool = false;
const CHECK_SELL_PRICE: bool = false;
const SHOW_BUY_DETAILS: bool = false;
const SHOW_SELL_DETAILS: bool = false;
const SHOW_MEAN: bool = false;

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

#[derive(PartialEq)]
enum TimeEnabler {
    Use(u64, String),
    Skip,
}

#[derive(PartialEq)]
enum KindOfTrade {
    Exported,
    Imported,
    Unknown,
}

struct GoodInfo {
    info: Good,
    buy_exchange_rate: f32,
    sell_exchange_rate: f32,
    initialization_qty: f32,
    kind_of_trade: KindOfTrade,
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
            Err(e) => panic!("Write to file error: {}\n", e),
        }
    }

    fn update_locks(&mut self) {
        let mut update_kind_price: Option<GoodKind> = None;

        // * Remove buy locks
        match &self.oldest_lock_buy_time {
            Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                if CHECK_IF_LOCK_BUY_DROPS {
                    eprintln!("Discard of oldest lock buy is occurring");
                }
                let lock = self.buy_locks.get(token).unwrap();
                let good = self
                    .good_data
                    .get_mut(&lock.locked_good.get_kind())
                    .unwrap();
                match good.info.merge(lock.locked_good.clone()) {
                    Ok(_) => (),
                    Err(e) => panic!(
                        "Different kind of goods in merge attempt @update_locks, details: {:?}",
                        e
                    ),
                }
                update_kind_price = Some(lock.locked_good.get_kind());
                self.buy_locks.remove(token);
                self.expired_tokens.insert(token.clone());
                self.active_buy_locks -= 1;
                self.oldest_lock_buy_time = if self.buy_locks.len() == 0 {
                    Skip
                } else {
                    let mut new_oldest = Skip;
                    for (token, good) in &self.buy_locks {
                        match new_oldest {
                            Use(time, _) if good.lock_time < time  => new_oldest = Use(good.lock_time, token.clone()),
                            Skip => new_oldest = Use(good.lock_time, token.clone()),
                            _ => (),
                        }
                    }
                    new_oldest
                }
            }
            Use(oldest, token) => {
                if CHECK_IF_LOCK_BUY_DROPS {
                    eprintln!(
                        "Oldest token: {} with time: {} ; current time: {}",
                        token, oldest, self.time
                    );
                }
            }
            _ => (),
        }

        update_kind_price = match update_kind_price {
            Some(kind) if kind != GoodKind::EUR => {
                self.update_good_price(kind);
                None
            }
            _ => None,
        };

        // * Remove sell locks
        match &self.oldest_lock_sell_time {
            Use(oldest, token) if oldest + MAX_LOCK_TIME < self.time => {
                if CHECK_IF_LOCK_SELL_DROPS {
                    eprintln!("Discard of oldest lock sell is occurring");
                }
                let lock = self.sell_locks.get(token).unwrap();
                let good = self.good_data.get_mut(&GoodKind::EUR).unwrap();

                match good.info.merge(lock.locked_eur.clone()) {
                    Ok(_) => (),
                    Err(e) => panic!(
                        "Different kind of goods in merge attempt @update_locks, details: {:?}",
                        e
                    ),
                }

                self.sell_locks.remove(token);
                self.expired_tokens.insert(token.clone());
                self.active_sell_locks -= 1;
                self.oldest_lock_sell_time = if self.sell_locks.len() == 0 {
                    Skip
                } else {
                    let mut new_oldest = Skip;
                    for (token, good) in &self.sell_locks {
                        match new_oldest {
                            Use(time, _) if good.lock_time < time => new_oldest = Use(good.lock_time, token.clone()),
                            Skip => new_oldest = Use(good.lock_time, token.clone()),
                            _ => (),
                        }
                    }
                    new_oldest
                }
            }
            Use(oldest, token) => {
                if CHECK_IF_LOCK_SELL_DROPS {
                    eprintln!(
                        "Oldest token: {} with time: {} ; current time: {}",
                        token, oldest, self.time
                    );
                }
            }
            _ => (),
        }

        match update_kind_price {
            Some(kind) if kind != GoodKind::EUR => self.update_good_price(kind),
            _ => (),
        };

        return;
    }

    fn increment_time(&mut self) {
        // * Managing the time overflow
        if self.time == std::u64::MAX {
            let mut shift_transactions = true;
            let mut oldest = std::u64::MAX;
       
            match (&self.oldest_lock_buy_time, &self.oldest_lock_sell_time){
                (Use(time1, _),Use(time2,_)) => oldest = std::cmp::min(*time1,*time2),
                (Use(time,_),_) | (_,Use(time,_)) => oldest = *time,
                (Skip,Skip) => shift_transactions = false,
            }
            
            if shift_transactions {
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
                self.expired_tokens.clear();
            }
            self.time -= oldest;
        }

        self.update_locks();
        self.time += 1;
        self.fluctuate_quantity();
    }

    // * This will try to rebalance all good quantities
    fn fluctuate_quantity(&mut self) {
        let mut rng = thread_rng();

        if rng.gen_range(0.0, 1.0) < PROBABILITY_OF_REBALANCE {
            let mut good_transformed_quantities: HashMap<GoodKind, f32> = HashMap::new();

            // * Calculate the mean to determine which good is suffering and which good is not
            let mut mean: f32 = 0.0;
            for (kind, good_info) in &mut self.good_data {
                if self.time % DURATION_OF_CHOSEN_KIND_OF_TRADE == 0 {
                    good_info.kind_of_trade = Unknown;
                }
                match *kind {
                    GoodKind::EUR => {
                        good_transformed_quantities.insert(GoodKind::EUR, good_info.info.get_qty())
                    }
                    GoodKind::USD => good_transformed_quantities.insert(
                        GoodKind::USD,
                        good_info.info.get_qty() * DEFAULT_USD_EUR_EXCHANGE_RATE,
                    ),
                    GoodKind::YEN => good_transformed_quantities.insert(
                        GoodKind::YEN,
                        good_info.info.get_qty() * DEFAULT_YEN_EUR_EXCHANGE_RATE,
                    ),
                    GoodKind::YUAN => good_transformed_quantities.insert(
                        GoodKind::YUAN,
                        good_info.info.get_qty() * DEFAULT_YUAN_EUR_EXCHANGE_RATE,
                    ),
                };
                mean += good_transformed_quantities[kind];
            }
            mean /= 4.0;

            if CHECK_IF_FLUCTUATION_OCCURS {
                eprintln!("Fluctuation is occurring with real time mean: {}", mean);
            }

            while let Some((
                (suffering_good_qty, suffering_good_kind),
                (eligible_good_qty, eligible_good_kind),
            )) = self.find_goods_to_balance(&good_transformed_quantities, mean)
            {
                if CHECK_IF_FIND_GOODS_TO_FLUCTUATE {
                    eprintln!("Before trading -> eligible good: {} with qty: {} ; suffering good: {} with qty: {}",
                    eligible_good_kind, eligible_good_qty, suffering_good_kind, suffering_good_qty)
                }

                if suffering_good_kind != GoodKind::EUR {
                    self.good_data
                        .get_mut(&suffering_good_kind)
                        .unwrap()
                        .kind_of_trade = Imported;
                }
                if eligible_good_kind != GoodKind::EUR {
                    self.good_data
                        .get_mut(&eligible_good_kind)
                        .unwrap()
                        .kind_of_trade = Exported;
                }

                let distance_to_fill =
                    f32::min(mean - suffering_good_qty, eligible_good_qty - mean);
                let split_from_eligible_good = match eligible_good_kind {
                    GoodKind::EUR => distance_to_fill,
                    GoodKind::USD => distance_to_fill * DEFAULT_EUR_USD_EXCHANGE_RATE,
                    GoodKind::YEN => distance_to_fill * DEFAULT_EUR_YEN_EXCHANGE_RATE,
                    GoodKind::YUAN => distance_to_fill * DEFAULT_EUR_YUAN_EXCHANGE_RATE,
                };
                let merge_to_suffering_good = match suffering_good_kind {
                    GoodKind::EUR => distance_to_fill,
                    GoodKind::USD => distance_to_fill * DEFAULT_EUR_USD_EXCHANGE_RATE,
                    GoodKind::YEN => distance_to_fill * DEFAULT_EUR_YEN_EXCHANGE_RATE,
                    GoodKind::YUAN => distance_to_fill * DEFAULT_EUR_YUAN_EXCHANGE_RATE,
                };

                self.good_data
                    .get_mut(&eligible_good_kind)
                    .unwrap()
                    .info
                    .split(split_from_eligible_good);
                self.good_data
                    .get_mut(&suffering_good_kind)
                    .unwrap()
                    .info
                    .merge(Good::new(suffering_good_kind, merge_to_suffering_good));

                *good_transformed_quantities
                    .get_mut(&eligible_good_kind)
                    .unwrap() -= distance_to_fill;
                *good_transformed_quantities
                    .get_mut(&suffering_good_kind)
                    .unwrap() += distance_to_fill;

                if CHECK_IF_FIND_GOODS_TO_FLUCTUATE {
                    eprintln!("After trading -> eligible good: {} with qty: {} ; suffering good: {} with qty: {}",
                    eligible_good_kind,good_transformed_quantities[&eligible_good_kind], suffering_good_kind, good_transformed_quantities[&suffering_good_kind])
                }
            }
        }
    }

    // * Retrieves which good is suffering and which one is eligible to transfer its quantities
    fn find_goods_to_balance(
        &self,
        good_transformed_quantities: &HashMap<GoodKind, f32>,
        mean: f32,
    ) -> Option<((f32, GoodKind), (f32, GoodKind))> {
        let (mut suffering_good, mut eligible_good): (
            Option<(f32, GoodKind)>,
            Option<(f32, GoodKind)>,
        ) = (None, None);

        for (kind, good_info) in &self.good_data {
            let good_qty = good_transformed_quantities[kind];
            if good_qty < mean && good_info.kind_of_trade != Exported {
                suffering_good = match suffering_good {
                    Some(good) if good_qty < good.0 => Some((good_qty, *kind)),
                    None => Some((good_qty, *kind)),
                    _ => suffering_good,
                };
            } else if good_qty > mean && good_info.kind_of_trade != Imported {
                eligible_good = match eligible_good {
                    Some(good) if good_qty > good.0 => Some((good_qty, *kind)),
                    None => Some((good_qty, *kind)),
                    _ => eligible_good,
                };
            }
        }

        suffering_good.zip(eligible_good)
    }

    fn update_good_price(&mut self, kind: GoodKind) {
        if let Some(good_info) = self.good_data.get_mut(&kind) {
            let default_price = match kind {
                GoodKind::USD => DEFAULT_USD_EUR_EXCHANGE_RATE,
                GoodKind::YEN => DEFAULT_YEN_EUR_EXCHANGE_RATE,
                GoodKind::YUAN => DEFAULT_YUAN_EUR_EXCHANGE_RATE,
                GoodKind::EUR => panic!("Eur should not update its price !"),
            };

            let (good_qty, initial_good_qty) = (
                good_info.info.get_qty() * default_price,
                good_info.initialization_qty * default_price,
            );

            if good_qty < self.mean * DEFAULT_PRICE_LOWER_BOUND_QTY_PERCENTAGE {
                good_info.buy_exchange_rate = (((1.0
                    - (good_qty - initial_good_qty * MINIMUM_GOOD_QUANTITY_PERCENTAGE)
                        / (self.mean - initial_good_qty * MINIMUM_GOOD_QUANTITY_PERCENTAGE))
                    * MAX_INFLATION_PRICE_INCREASE_PERCENTAGE)
                    + 1.0)
                    * default_price
            } else if good_qty < self.mean * FIRST_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE {
                good_info.buy_exchange_rate = default_price
            } else if good_qty < self.mean * SECOND_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE {
                good_info.buy_exchange_rate = default_price * FIRST_DEFLATION_PRICE_DISCOUNT
            } else if good_qty < self.mean * THIRD_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE {
                good_info.buy_exchange_rate = default_price * SECOND_DEFLATION_PRICE_DISCOUNT
            } else if good_qty < self.mean * MAX_DEFLATION_PRICE_LOWER_BOUND_QTY_PERCENTAGE {
                good_info.buy_exchange_rate = default_price * THIRD_DEFLATION_PRICE_DISCOUNT
            } else {
                good_info.buy_exchange_rate = default_price * MAX_DEFLATION_PRICE_DISCOUNT
            }
            good_info.sell_exchange_rate = good_info.buy_exchange_rate * BUY_TO_SELL_PERCENTAGE
        } else {
            panic!(
                "Couldn't find GoodKind key {} in the good_data HashMap",
                kind
            )
        }
    }

    // * Notify other markets
    fn notify_markets(&mut self, event: Event) {
        for m in &mut self.subscribers {
            m.on_event(event.clone())
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
    fn on_event(&mut self, _event: Event) {
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
        let mut good_kinds = vec![GoodKind::USD, GoodKind::YUAN, GoodKind::YEN];
        let mut rng = thread_rng();

        eur = rng.gen_range(
            max * EUR_LOWER_BOUND_INIT_PERCENTAGE,
            max * EUR_UPPER_BOUND_INIT_PERCENTAGE,
        );
        max -= eur;

        good_kinds.shuffle(&mut rng);

        let mut random_qty = rng.gen_range(
            max * SECOND_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
            max * SECOND_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
        );
        match good_kinds[0] {
            GoodKind::USD => usd = random_qty,
            GoodKind::YEN => yen = random_qty,
            GoodKind::YUAN => yuan = random_qty,
            GoodKind::EUR => panic!("Matched EUR which has been already initialized !"),
        }
        max -= random_qty;

        random_qty = rng.gen_range(
            max * THIRD_GOOD_LOWER_BOUND_INIT_PERCENTAGE,
            max * THIRD_GOOD_UPPER_BOUND_INIT_PERCENTAGE,
        );
        match good_kinds[1] {
            GoodKind::USD => usd = random_qty,
            GoodKind::YEN => yen = random_qty,
            GoodKind::YUAN => yuan = random_qty,
            GoodKind::EUR => panic!("Matched EUR which has been already initialized !"),
        }
        max -= random_qty;

        match good_kinds[2] {
            GoodKind::USD => usd = max,
            GoodKind::YEN => yen = max,
            GoodKind::YUAN => yuan = max,
            GoodKind::EUR => panic!("Matched EUR which has been already initialized !"),
        }
        Self::new_with_quantities(
            eur,
            yen * DEFAULT_EUR_YEN_EXCHANGE_RATE,
            usd * DEFAULT_EUR_USD_EXCHANGE_RATE,
            yuan * DEFAULT_EUR_YUAN_EXCHANGE_RATE,
        )
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
            mean: (usd * DEFAULT_USD_EUR_EXCHANGE_RATE
                + yen * DEFAULT_YEN_EUR_EXCHANGE_RATE
                + yuan * DEFAULT_YUAN_EUR_EXCHANGE_RATE)
                / 3.0,
            good_data: HashMap::new(),
            buy_locks: HashMap::new(),
            sell_locks: HashMap::new(),
            subscribers: Vec::new(),
            log_file: file,
            expired_tokens: HashSet::new(),
        };

        if SHOW_MEAN {
            eprintln!("initialization_mean : {}", market.mean);
        }

        market.good_data.insert(
            GoodKind::EUR,
            GoodInfo {
                info: Good::new(GoodKind::EUR, eur),
                buy_exchange_rate: 1.0,
                sell_exchange_rate: 1.0,
                initialization_qty: eur,
                kind_of_trade: Unknown,
            },
        );

        market.good_data.insert(
            GoodKind::USD,
            GoodInfo {
                info: Good::new(GoodKind::USD, usd),
                buy_exchange_rate: 0.0,
                sell_exchange_rate: 0.0,
                initialization_qty: usd,
                kind_of_trade: Unknown,
            },
        );

        market.good_data.insert(
            GoodKind::YEN,
            GoodInfo {
                info: Good::new(GoodKind::YEN, yen),
                buy_exchange_rate: 0.0,
                sell_exchange_rate: 0.0,
                initialization_qty: yen,
                kind_of_trade: Unknown,
            },
        );

        market.good_data.insert(
            GoodKind::YUAN,
            GoodInfo {
                info: Good::new(GoodKind::YUAN, yuan),
                buy_exchange_rate: 0.0,
                sell_exchange_rate: 0.0,
                initialization_qty: yuan,
                kind_of_trade: Unknown,
            },
        );

        market.update_good_price(GoodKind::USD);
        market.update_good_price(GoodKind::YEN);
        market.update_good_price(GoodKind::YUAN);
        market.write_on_log_file(log_format_market_init!(NAME, eur, usd, yen, yuan));

        Rc::new(RefCell::new(market))
    }

    fn new_file(_path: &str) -> Rc<RefCell<dyn Market>>
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

        let good_data = &self.good_data[&kind];

        // * Getting the quantity availability
        let available_good_qty = good_data.info.get_qty();
        let quantity_cap = good_data.initialization_qty * MINIMUM_GOOD_QUANTITY_PERCENTAGE;

        if available_good_qty - quantity < quantity_cap {
            return Err(MarketGetterError::InsufficientGoodQuantityAvailable {
                requested_good_kind: kind,
                requested_good_quantity: quantity,
                available_good_quantity: available_good_qty,
            });
        }

        if CHECK_BUY_PRICE {
            eprintln!(
                "Requested good {} with qty: {}, and exchange rate: {}",
                kind, quantity, good_data.buy_exchange_rate
            );
        }

        let mut good_price = good_data.buy_exchange_rate * quantity;

        // * Apply the discount
        if quantity >= FIRST_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY * available_good_qty {
            if quantity < SECOND_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY * available_good_qty {
                good_price *= FIRST_LOCK_BUY_DISCOUNT;
            } else if quantity < THIRD_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY * available_good_qty {
                good_price *= SECOND_LOCK_BUY_DISCOUNT;
            } else if quantity < MAX_LOCK_BUY_DISCOUNT_LOWER_BOUND_QTY * available_good_qty {
                good_price *= THIRD_LOCK_BUY_DISCOUNT;
            } else {
                good_price *= MAX_LOCK_BUY_DISCOUNT;
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
        let available_eur_qty = eur_data.info.get_qty();
        let quantity_cap = eur_data.initialization_qty * MINIMUM_EUR_QUANTITY_PERCENTAGE;
        let price = self.good_data[&kind].sell_exchange_rate * quantity;

        if available_eur_qty - price < quantity_cap {
            return Err(MarketGetterError::InsufficientGoodQuantityAvailable {
                requested_good_kind: GoodKind::EUR,
                requested_good_quantity: price,
                available_good_quantity: available_eur_qty,
            });
        }

        if CHECK_SELL_PRICE {
            eprintln!(
                "Requested good {} with qty: {}, and exchange rate: {}",
                kind, quantity, self.good_data[&kind].sell_exchange_rate
            );
        }

        Ok(price)
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
        let good_price = match self.get_buy_price(kind_to_buy, quantity_to_buy) {
            Ok(price) => price,
            Err(error) => {
                self.write_on_log_file(log_format_lock_buy!(
                    NAME,
                    trader_name,
                    kind_to_buy,
                    quantity_to_buy,
                    bid
                ));
                match error {
                    MarketGetterError::NonPositiveQuantityAsked => {
                        return Err(LockBuyError::NonPositiveQuantityToBuy {
                            negative_quantity_to_buy: quantity_to_buy,
                        })
                    }
                    MarketGetterError::InsufficientGoodQuantityAvailable {
                        requested_good_kind,
                        requested_good_quantity,
                        available_good_quantity,
                    } => {
                        return Err(LockBuyError::InsufficientGoodQuantityAvailable {
                            requested_good_kind: requested_good_kind,
                            requested_good_quantity: requested_good_quantity,
                            available_good_quantity: available_good_quantity,
                        })
                    }
                }
            }
        };

        // * Non positive bid
        if bid <= 0.0 {
            self.write_on_log_file(log_format_lock_buy!(
                NAME,
                trader_name,
                kind_to_buy,
                quantity_to_buy,
                bid
            ));
            return Err(LockBuyError::NonPositiveBid { negative_bid: bid });
        }

        // * Max locks reached
        if self.active_buy_locks == MAX_LOCK_BUY_NUM {
            self.write_on_log_file(log_format_lock_buy!(
                NAME,
                trader_name,
                kind_to_buy,
                quantity_to_buy,
                bid
            ));
            return Err(LockBuyError::MaxAllowedLocksReached);
        }

        // * Bid too low
        if bid < good_price {
            self.write_on_log_file(log_format_lock_buy!(
                NAME,
                trader_name,
                kind_to_buy,
                quantity_to_buy,
                bid
            ));
            return Err(LockBuyError::BidTooLow {
                requested_good_kind: kind_to_buy,
                requested_good_quantity: quantity_to_buy,
                low_bid: bid,
                lowest_acceptable_bid: good_price,
            });
        }

        // * Create a new buy transaction token
        token = BVCMarket::token(String::from("lock_buy"), trader_name.clone(), self.time);

        if self.oldest_lock_buy_time == Skip {
            self.oldest_lock_buy_time = Use(self.time, token.clone())
        }

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
            if kind_to_buy != GoodKind::EUR {
                self.update_good_price(kind_to_buy);
            }
            self.write_on_log_file(log_format_lock_buy!(
                NAME,
                trader_name,
                kind_to_buy,
                quantity_to_buy,
                bid,
                token
            ));
            Ok(token)
        } else {
            panic!("Missing key: {} in good_data ", kind_to_buy)
        }
    }

    fn buy(&mut self, token: String, cash: &mut Good) -> Result<Good, BuyError> {
        if !self.buy_locks.contains_key(&token) {
            // * Check if the good has expired
            if self.expired_tokens.contains(&token) {
                self.write_on_log_file(log_format_buy!(NAME, token, Err()));
                return Err(BuyError::ExpiredToken {
                    expired_token: token,
                });
            } else {
                // * Otherwise it is an invalid token
                self.write_on_log_file(log_format_buy!(NAME, token, Err()));
                return Err(BuyError::UnrecognizedToken {
                    unrecognized_token: token,
                });
            }
        }

        // * Invalid cash kind
        if cash.get_kind() != GoodKind::EUR {
            self.write_on_log_file(log_format_buy!(NAME, token, Err()));
            return Err(BuyError::GoodKindNotDefault {
                non_default_good_kind: cash.get_kind(),
            });
        }

        // * Insufficient good quantity
        if self.buy_locks[&token].buy_price > cash.get_qty() {
            self.write_on_log_file(log_format_buy!(NAME, token, Err()));
            return Err(BuyError::InsufficientGoodQuantity {
                contained_quantity: cash.get_qty(),
                pre_agreed_quantity: self.buy_locks[&token].buy_price,
            });
        }

        // * Merge the eur good from the trader, notify other markets and return the locked good
        let eur_to_pay = self.buy_locks[&token].buy_price;
        let locked_good = self.buy_locks[&token].locked_good.clone(); // * There was a clone here
        self.buy_locks.remove(&token);
        
        // * Updates the oldest lock if bought one was the oldest
        match &self.oldest_lock_buy_time {
            Use(_,tok) if *tok == token => {
                let mut oldest_lock = Skip;
                for (tk,good_lock) in &self.buy_locks{
                    let cmp_time = good_lock.lock_time;
                    match oldest_lock {
                        Use(time,_) if cmp_time < time => oldest_lock = Use(cmp_time,tk.clone()),
                        Skip => oldest_lock = Use(cmp_time,tk.clone()),
                        _ => (),
                    }
                }
                self.oldest_lock_buy_time = oldest_lock;
            },
            _ => (),
        }

        self.active_buy_locks -= 1;
        if let Some(eur) = self.good_data.get_mut(&GoodKind::EUR) {
            if SHOW_BUY_DETAILS {
                eprintln!(
                    "Adding {} euros to wallet eur {}",
                    eur_to_pay,
                    eur.info.get_qty()
                );
            }

            eur.info.merge(cash.split(eur_to_pay).unwrap());
            self.notify_markets(Event {
                kind: EventKind::Bought,
                good_kind: locked_good.get_kind(),
                quantity: locked_good.get_qty(),
                price: eur_to_pay,
            });

            self.increment_time();
            self.write_on_log_file(log_format_buy!(NAME, token, Ok()));
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
        let good_price = match self.get_sell_price(kind_to_sell, quantity_to_sell) {
            Ok(price) => price,
            Err(error) => {
                self.write_on_log_file(log_format_lock_sell!(
                    NAME,
                    trader_name,
                    kind_to_sell,
                    quantity_to_sell,
                    offer
                ));
                match error {
                    MarketGetterError::NonPositiveQuantityAsked => {
                        return Err(LockSellError::NonPositiveQuantityToSell {
                            negative_quantity_to_sell: quantity_to_sell,
                        })
                    }
                    MarketGetterError::InsufficientGoodQuantityAvailable {
                        requested_good_kind,
                        requested_good_quantity,
                        available_good_quantity,
                    } => {
                        return Err(LockSellError::InsufficientDefaultGoodQuantityAvailable {
                            offered_good_kind: requested_good_kind,
                            offered_good_quantity: requested_good_quantity,
                            available_good_quantity: available_good_quantity,
                        })
                    }
                }
            }
        };

        // * Non positive offer
        if offer <= 0.0 {
            self.write_on_log_file(log_format_lock_sell!(
                NAME,
                trader_name,
                kind_to_sell,
                quantity_to_sell,
                offer
            ));
            return Err(LockSellError::NonPositiveOffer {
                negative_offer: offer,
            });
        }

        // * Max lock reached
        if self.active_sell_locks == MAX_LOCK_SELL_NUM {
            self.write_on_log_file(log_format_lock_sell!(
                NAME,
                trader_name,
                kind_to_sell,
                quantity_to_sell,
                offer
            ));
            return Err(LockSellError::MaxAllowedLocksReached);
        }

        //* Offer too high
        if offer > good_price {
            self.write_on_log_file(log_format_lock_sell!(
                NAME,
                trader_name,
                kind_to_sell,
                quantity_to_sell,
                offer
            ));
            return Err(LockSellError::OfferTooHigh {
                offered_good_kind: kind_to_sell,
                offered_good_quantity: quantity_to_sell,
                high_offer: offer,
                highest_acceptable_offer: good_price,
            });
        }

        token = BVCMarket::token(String::from("lock_sell"), trader_name.clone(), self.time);

        if self.oldest_lock_sell_time == Skip {
            self.oldest_lock_sell_time = Use(self.time, token.clone())
        }

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
            if kind_to_sell != GoodKind::EUR {
                self.update_good_price(kind_to_sell);
            }
            self.write_on_log_file(log_format_lock_sell!(
                NAME,
                trader_name,
                kind_to_sell,
                quantity_to_sell,
                offer,
                token
            ));
            return Ok(token);
        } else {
            panic!("Missing key: GoodKind::EUR in good_data ")
        }
    }

    fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {
        if !self.sell_locks.contains_key(&token) {
            // * Check if the good has expired
            if self.expired_tokens.contains(&token) {
                self.write_on_log_file(log_format_sell!(NAME, token, Err()));
                return Err(SellError::ExpiredToken {
                    expired_token: token,
                });
            } else {
                // * Otherwise it is an invalid token
                self.write_on_log_file(log_format_sell!(NAME, token, Err()));
                return Err(SellError::UnrecognizedToken {
                    unrecognized_token: token,
                });
            }
        }

        // * Kind of goods not matching
        if self.sell_locks[&token].locked_kind != good.get_kind() {
            self.write_on_log_file(log_format_sell!(NAME, token, Err()));
            return Err(SellError::WrongGoodKind {
                wrong_good_kind: good.get_kind(),
                pre_agreed_kind: self.sell_locks[&token].locked_kind,
            });
        }

        //* Quantity of goods not matching
        if good.get_qty() < self.sell_locks[&token].receiving_good_qty {
            self.write_on_log_file(log_format_sell!(NAME, token, Err()));
            return Err(SellError::InsufficientGoodQuantity {
                contained_quantity: good.get_qty(),
                pre_agreed_quantity: self.sell_locks[&token].receiving_good_qty,
            });
        }

        // * Merge the good from the trader, notify other markets and return the locked eur pre agreed quantity
        let lock_info = self.sell_locks[&token].clone();
        self.sell_locks.remove(&token);

        // * Updates the oldest lock if bought one was the oldest
        match &self.oldest_lock_buy_time {
            Use(_,tok) if *tok == token => {
                let mut oldest_lock = Skip;
                for (tk,good_lock) in &self.sell_locks{
                    let cmp_time = good_lock.lock_time;
                    match oldest_lock {
                        Use(time,_) if cmp_time < time => oldest_lock = Use(cmp_time,tk.clone()),
                        Skip => oldest_lock = Use(cmp_time,tk.clone()),
                        _ => (),
                    }
                }
                self.oldest_lock_buy_time = oldest_lock;
            },
            _ => (),
        }

        self.active_sell_locks -= 1;
        if let Some(good_to_fill) = self.good_data.get_mut(&good.get_kind()) {
            if SHOW_SELL_DETAILS {
                eprintln!(
                    "Adding {} {} to wallet {} {}",
                    lock_info.receiving_good_qty,
                    lock_info.locked_kind,
                    lock_info.locked_kind,
                    good_to_fill.info.get_qty()
                );
            }

            good_to_fill
                .info
                .merge(good.split(lock_info.receiving_good_qty).unwrap());

            self.notify_markets(Event {
                kind: EventKind::Sold,
                good_kind: good.get_kind(),
                quantity: lock_info.receiving_good_qty,
                price: lock_info.locked_eur.get_qty(),
            });

            self.increment_time();
            if lock_info.locked_kind != GoodKind::EUR {
                self.update_good_price(lock_info.locked_kind);
            }
            self.write_on_log_file(log_format_sell!(NAME, token, Ok()));
            Ok(lock_info.locked_eur)
        } else {
            panic!("Missing key: {} in good_data ", good.get_kind())
        }
    }
}
