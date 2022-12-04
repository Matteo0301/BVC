pub mod BVC {
    use std::{cell::RefCell, rc::Rc, collections::HashMap};
    use unitn_market_2022::{
        event::{event::Event, notifiable::Notifiable},
        good::{consts::STARTING_CAPITAL, good::Good, good_kind::GoodKind},
        market::{
            good_label::GoodLabel, BuyError, LockBuyError, Market, MarketGetterError, SellError,
        },
    };
    use TimeEnabler::{Skip,Use};

    use rand::thread_rng;
    use rand::Rng;

    const NAME : &'static str = "BVC";
    const MAX_LOCK_TIME : i32 = 5;
    const MAX_LOCK_BUY_NUM : i32 = 4;
    const MAX_LOCK_SELL_NUM : i32 = 4;

    pub struct BVCMarket {
        /* eur: Good,
        usd: Good,
        yen: Good,
        yuan: Good, */
        time : u32, // needs to be reset before reaching MAX_INT and change transaction times accordingly
        oldest_lock_buy_time : TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        oldest_lock_sell_time : TimeEnabler, //used to avoid iterating the map when useless, set to Skip to ignore
        good_data : HashMap<GoodKind, GoodInfo>,
        buy_locks : HashMap<String,LockBuyGood>,
        sell_locks : HashMap<String,LockSellGood>,
        subscribers : Vec<Box<dyn Notifiable>>,
    }

    enum TimeEnabler {
        Use(u32),
        Skip,
    }

    struct GoodInfo{
        info : Good,
        buy_exchange_rate : f32,
        sell_exchange_rate : f32,
    }

    struct LockBuyGood{
        locked_good : Good,
        buy_price : f32,
        lock_time : i32,
    }

    struct LockSellGood{
        locked_quantity : f32,
        sell_price : f32,
        lock_time : i32
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
            eur = rng.gen_range(0.0, max);
            max -= eur;
            yen = rng.gen_range(0.0, max);
            max -= yen;
            usd = rng.gen_range(0.0, max);
            max -= usd;
            yuan = rng.gen_range(0.0, max);
            max -= yuan;
            Self::new_with_quantities(eur, yen, usd, yuan)
        }

        fn new_with_quantities(eur: f32, yen: f32, usd: f32, yuan: f32) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
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
                time : 0,
                oldest_lock_buy_time : Skip,
                oldest_lock_sell_time : Skip,
                good_data : HashMap::new(), // Implement starting prices here
                buy_locks : HashMap::new(),
                sell_locks : HashMap::new(),
                subscribers : Vec::new(),
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

        fn get_budget(&self) -> f32 { self.good_data[&GoodKind::EUR].info.get_qty() }

        fn get_buy_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {
            if quantity.is_sign_negative(){
                return Err(MarketGetterError::NonPositiveQuantityAsked);
            }
            // Discounts to be implemented, i.e.: if the trader wants to buy more than 50% of the market good, i will apply a scalable discount
            // Remember if the kind is EUROS we must change it 1:1
            match kind{
                GoodKind::EUR => Ok(quantity),
                _ => Ok(self.good_data[&kind].buy_exchange_rate * quantity)
            }
        }

        fn get_sell_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {
            if quantity.is_sign_negative() {
                return Err(MarketGetterError::NonPositiveQuantityAsked);
            }
            // Discounts to be implemented, i.e.: if the trader wants to sell more than 50% of the good (percentage relative to the quantity of the Market not the Trader's one) i will apply a (less) scalable discount
            // Remember if the kind is EUROS we must change it 1:1
            match kind{
                GoodKind::EUR => Ok(quantity),
                _ => Ok(self.good_data[&kind].sell_exchange_rate * quantity)
            }
        }

        fn get_goods(&self) -> Vec<GoodLabel> {
            let prices : Vec<GoodLabel> = Vec::new();
            for (kind,data) in self.good_data {
                prices.push(GoodLabel{
                    good_kind : kind, // Does this copy or move ??
                    quantity : data.info.get_qty(),
                    exchange_rate_buy : data.buy_exchange_rate,
                    exchange_rate_sell : data.sell_exchange_rate,
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
        ) -> Result<String, LockBuyError> {
        }

        fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {}
    }
}
