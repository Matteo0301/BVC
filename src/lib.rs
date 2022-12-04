pub mod bvc {
    use std::rc::Rc;

    use unitn_market_2022::{
        event::{event::Event, notifiable::Notifiable},
        good::good::Good,
        market::Market,
    };

    const NAME: &'static str = "BVC";

    pub struct BVC_market {
        eur: Good,
        usd: Good,
        yen: Good,
        yuan: Good,
    }

    impl Notifiable for BVC_market {
        fn add_subscriber(&mut self, subscriber: Box<dyn Notifiable>) {}
        fn on_event(&mut self, event: Event) {}
    }

    impl Market for BVC_market {
        fn new_random() -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
        }
        fn new_with_quantities(eur: f32, yen: f32, usd: f32, yuan: f32) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
        }
        fn new_file(path: &str) -> Rc<RefCell<dyn Market>>
        where
            Self: Sized,
        {
        }
        fn get_name(&self) -> &'static str {}

        fn get_budget(&self) -> f32 {}

        fn get_buy_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {}

        fn get_sell_price(&self, kind: GoodKind, quantity: f32) -> Result<f32, MarketGetterError> {}

        fn get_goods(&self) -> Vec<GoodLabel> {}

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
        }

        fn sell(&mut self, token: String, good: &mut Good) -> Result<Good, SellError> {}
    }
}
