macro_rules! log_format_name_and_time {
    ($name:expr) => {
        format!("{}|{}|",$name,Utc::now().naive_local().format("%Y:%m:%d:%H:%M:%S:%3f"))
    };
}

macro_rules! log_format_market_init {
    ($name:expr,$eur:expr, $usd:expr, $yen:expr, $yuan:expr) => {
        format!("-----\n{}MARKET INITIALIZATION\nEUR: {:+e}\nUSD: {:+e}\nYEN: {:+e}\nYUAN: {:+e}\nEND MARKET INITIALIZATION\n\n",log_format_name_and_time!($name), $eur, $usd, $yen, $yuan)
    };
}

macro_rules! log_format_lock_buy {
    ($name:expr,$trader:expr,$kind:expr,$qty:expr,$bid:expr,$token:expr) => {
        format!("{}LOCK_BUY-{}-KIND_TO_BUY:{}-QUANTITY_TO_BUY:{}-BID:{}-TOKEN:{}\n",log_format_name_and_time!($name),$trader,$kind,$qty,$bid,$token)
    };
    ($name:expr,$trader:expr,$kind:expr,$qty:expr,$bid:expr) => {
        format!("{}LOCK_BUY-{}-KIND_TO_BUY:{}-QUANTITY_TO_BUY:{}-BID:{}-ERROR\n",log_format_name_and_time!($name),$trader,$kind,$qty,$bid)
    };
}

macro_rules! log_format_lock_sell {
    ($name:expr,$trader:expr,$kind:expr,$qty:expr,$offer:expr,$token:expr) => {
        format!("{}LOCK_SELL-{}-KIND_TO_SELL:{}-QUANTITY_TO_SELL:{}-OFFER:{}-TOKEN:{}\n",log_format_name_and_time!($name),$trader,$kind,$qty,$offer,$token)
    };
    ($name:expr,$trader:expr,$kind:expr,$qty:expr,$offer:expr) => {
        format!("{}LOCK_SELL-{}-KIND_TO_SELL:{}-QUANTITY_TO_SELL:{}-OFFER:{}-ERROR\n",log_format_name_and_time!($name),$trader,$kind,$qty,$offer)
    };
}

macro_rules! log_format_buy {
    ($name:expr,$token:expr,Ok()) => {
        format!("{}BUY-TOKEN:{}-OK\n",log_format_name_and_time!($name), $token)
    };
    ($name:expr,$token:expr,Err()) => {
        format!("{}BUY-TOKEN:{}-ERROR\n",log_format_name_and_time!($name), $token)
    };
}

macro_rules! log_format_sell {
    ($name:expr,$token:expr,Ok()) => {
        format!("{}SELL-TOKEN:{}-OK\n", log_format_name_and_time!($name),$token)
    };
    ($name:expr,$token:expr,Err()) => {
        format!("{}SELL-TOKEN:{}-ERROR\n",log_format_name_and_time!($name), $token)
    };
}